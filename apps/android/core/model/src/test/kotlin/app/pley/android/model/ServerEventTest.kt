package app.pley.android.model

import kotlinx.serialization.SerializationException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ServerEventTest {
    @Test
    fun knownEventsRead() {
        assertEquals(ServerEvent.ScanStarted(1), ServerEvent.parse("""{"type":"scan_started","library_id":1}"""))
        assertEquals(
            ServerEvent.ScanProgress(1, ScanPhase.Probing, 2, 3),
            ServerEvent.parse("""{"type":"scan_progress","library_id":1,"phase":"probing","processed":2,"total":3}"""),
        )
        assertEquals(
            ServerEvent.ScanFinished(2, added = 4, updated = 1, removed = 0),
            ServerEvent.parse("""{"type":"scan_finished","library_id":2,"added":4,"updated":1,"removed":0}"""),
        )
        assertEquals(ServerEvent.LibraryChanged(3), ServerEvent.parse("""{"type":"library_changed","library_id":3}"""))
        assertEquals(ServerEvent.LibrariesChanged, ServerEvent.parse("""{"type":"libraries_changed"}"""))
        assertEquals(ServerEvent.SessionsChanged, ServerEvent.parse("""{"type":"sessions_changed"}"""))
        assertEquals(
            ServerEvent.SessionStopped("s1", "Server maintenance", byAdmin = true),
            ServerEvent.parse("""{"type":"session_stopped","session_id":"s1","message":"Server maintenance","by_admin":true}"""),
        )
        assertEquals(ServerEvent.SessionStopped("s2", null, false), ServerEvent.parse("""{"type":"session_stopped","session_id":"s2","message":null,"by_admin":false}"""))
        assertEquals(ServerEvent.WatchlistChanged, ServerEvent.parse("""{"type":"watchlist_changed"}"""))
        assertEquals(
            ServerEvent.UserStateChanged(listOf(203, 202, 201), children = false),
            ServerEvent.parse("""{"type":"user_state_changed","item_ids":[203,202,201],"children":false}"""),
        )
        assertEquals(ServerEvent.UserStateChanged(listOf(201), children = true), ServerEvent.parse("""{"type":"user_state_changed","item_ids":[201],"children":true}"""))
        assertEquals(ServerEvent.LiveChannelsChanged, ServerEvent.parse("""{"type":"live_channels_changed"}"""))
        assertEquals(ServerEvent.LiveTunersChanged, ServerEvent.parse("""{"type":"live_tuners_changed"}"""))
    }

    @Test
    fun requestEventsRead() {
        assertEquals(
            ServerEvent.RequestsChanged(42, TitleKind.Show, 1396),
            ServerEvent.parse("""{"type":"requests_changed","request_id":42,"kind":"show","tmdb_id":1396}"""),
        )
        // Many changed: all three null.
        assertEquals(ServerEvent.RequestsChanged(), ServerEvent.parse("""{"type":"requests_changed","request_id":null,"kind":null,"tmdb_id":null}"""))
        assertEquals(
            ServerEvent.RequestAvailable(42, "Breaking Bad", TitleKind.Show, 1396, itemId = 230, seasons = listOf(2), complete = false),
            ServerEvent.parse(
                """{"type":"request_available","request_id":42,"title":"Breaking Bad","kind":"show","tmdb_id":1396,"item_id":230,"poster_url":null,"seasons":[2],"complete":false}""",
            ),
        )
        assertEquals(
            ServerEvent.RequestDecided(46, "Alien: Romulus", TitleKind.Movie, 945961, RequestStatus.Declined, null),
            ServerEvent.parse("""{"type":"request_decided","request_id":46,"title":"Alien: Romulus","kind":"movie","tmdb_id":945961,"status":"declined","reason":null}"""),
        )
        val created = ServerEvent.parse(
            """{"type":"request_created","request_id":49,"title":"Arcane","kind":"show","tmdb_id":94605,"seasons":[1,2],"requested_by":{"id":3,"name":"Kit","avatar_url":null,"avatar_color":"#a3e635"},"status":"pending"}""",
        ) as ServerEvent.RequestCreated
        assertEquals("Kit", created.requestedBy?.name)
        assertEquals(RequestStatus.Pending, created.status)
        assertEquals(
            ServerEvent.RequestFailed(50, "Inside Out 2", TitleKind.Movie, "Radarr refused the key", null),
            ServerEvent.parse("""{"type":"request_failed","request_id":50,"title":"Inside Out 2","kind":"movie","reason":"Radarr refused the key","retry_at":null}"""),
        )
        assertEquals(ServerEvent.RequestsSettingsChanged, ServerEvent.parse("""{"type":"requests_settings_changed"}"""))
        for (event in listOf(created, ServerEvent.RequestsSettingsChanged, ServerEvent.RequestsChanged(1, TitleKind.Movie, 2))) {
            assertEquals(event, ServerEvent.parse(PleyJson.encodeToString(ServerEvent.serializer(), event)))
        }
    }

    @Test
    fun castEventsRead() {
        assertEquals(ServerEvent.CastDevicesChanged, ServerEvent.parse("""{"type":"cast_devices_changed"}"""))
        val heard = ServerEvent.parse(
            """{"type":"cast_state","device_id":"tv-1","state":{"item":{"id":101,"library_id":1,"kind":"movie","title":"A movie"},""" +
                """"position_ms":61000,"duration_ms":7200000,"state":"paused","age_ms":250}}""",
        ) as ServerEvent.CastStateChanged
        assertEquals("tv-1", heard.deviceId)
        assertEquals(101L, heard.state?.item?.id)
        assertEquals(CastPlayState.Paused, heard.state?.state)
        assertEquals(250L, heard.state?.ageMs)
        assertEquals(ServerEvent.CastStateChanged("tv-1", null), ServerEvent.parse("""{"type":"cast_state","device_id":"tv-1","state":null}"""))
        assertEquals(
            ServerEvent.CastPlay("tv-1", 203, fileId = 9, positionMs = null),
            ServerEvent.parse("""{"type":"cast_play","device_id":"tv-1","item_id":203,"file_id":9,"position_ms":null}"""),
        )
        assertEquals(
            ServerEvent.CastControl("tv-1", CastAction.Seek, 90_000),
            ServerEvent.parse("""{"type":"cast_control","device_id":"tv-1","action":"seek","position_ms":90000}"""),
        )
        // An action a newer server adds is kept as Unknown, and ignored by the player.
        assertEquals(ServerEvent.CastControl("tv-1", CastAction.Unknown), ServerEvent.parse("""{"type":"cast_control","device_id":"tv-1","action":"rewind"}"""))
        for (event in listOf(heard, ServerEvent.CastDevicesChanged, ServerEvent.CastPlay("tv-1", 101), ServerEvent.CastControl("tv-1", CastAction.Stop))) {
            assertEquals(event, ServerEvent.parse(PleyJson.encodeToString(ServerEvent.serializer(), event)))
        }
    }

    @Test
    fun castBodiesWriteWhatTheServerReads() {
        assertEquals("""{"item_id":101,"position_ms":0}""", PleyJson.encodeToString(CastPlayRequest.serializer(), CastPlayRequest(101, positionMs = 0)))
        assertEquals("""{"action":"pause"}""", PleyJson.encodeToString(CastControlRequest.serializer(), CastControlRequest(CastAction.Pause)))
        assertEquals(
            """{"item_id":101,"position_ms":5000,"duration_ms":60000,"state":"buffering"}""",
            PleyJson.encodeToString(CastReport.serializer(), CastReport(101, 5_000, 60_000, CastPlayState.Buffering)),
        )
        // Playing nothing: the server reads the missing item_id as null.
        assertEquals("""{"position_ms":0,"state":"playing"}""", PleyJson.encodeToString(CastReport.serializer(), CastReport()))
    }

    @Test
    fun castNamesAreWhatTheServerTakes() {
        assertEquals("Living room", CastIdentity.nameOf("  Living room ", "TV"))
        assertEquals("TV", CastIdentity.nameOf(" ", "TV"))
        assertEquals(CastIdentity.NAME_MAX, CastIdentity.nameOf("x".repeat(80), "TV").length)
    }

    @Test
    fun unknownTypesAreKeptByName() {
        val event = ServerEvent.parse("""{"type":"markers_found","item_id":203}""")
        assertEquals(ServerEvent.Unknown("markers_found"), event)
        assertNull(event.libraryId)
        assertEquals(ServerEvent.Unknown(""), ServerEvent.parse("""{"library_id":1}"""))
    }

    @Test
    fun newFieldsAndPhasesDontBreakKnownEvents() {
        val event = ServerEvent.parse("""{"type":"scan_progress","library_id":1,"phase":"fingerprinting","processed":2,"total":3,"eta_ms":9}""")
        assertEquals(ServerEvent.ScanProgress(1, ScanPhase.Unknown, 2, 3), event)
        assertEquals(1L, event.libraryId)
    }

    @Test(expected = SerializationException::class)
    fun textThatIsntAnObjectFails() {
        ServerEvent.parse("""["scan_started"]""")
    }

    @Test
    fun eventsWriteWhatTheServerSends() {
        assertEquals(
            """{"type":"scan_progress","library_id":1,"phase":"probing","processed":2,"total":3}""",
            PleyJson.encodeToString(ServerEvent.serializer(), ServerEvent.ScanProgress(1, ScanPhase.Probing, 2, 3)),
        )
        assertEquals("""{"type":"libraries_changed"}""", PleyJson.encodeToString(ServerEvent.serializer(), ServerEvent.LibrariesChanged))
        assertEquals(
            """{"type":"user_state_changed","item_ids":[203,202,201],"children":false}""",
            PleyJson.encodeToString(ServerEvent.serializer(), ServerEvent.UserStateChanged(listOf(203, 202, 201))),
        )
        val events = listOf(
            ServerEvent.ScanStarted(4),
            ServerEvent.SessionsChanged,
            ServerEvent.WatchlistChanged,
            ServerEvent.UserStateChanged(listOf(201), children = true),
            ServerEvent.LiveChannelsChanged,
            ServerEvent.LiveTunersChanged,
            ServerEvent.Unknown("later"),
        )
        for (event in events) {
            assertEquals(event, ServerEvent.parse(PleyJson.encodeToString(ServerEvent.serializer(), event)))
        }
    }
}
