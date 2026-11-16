package app.pley.android.ui.phone.admin.dashboard

import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextReplacement
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.admin.DashboardSession
import app.pley.android.network.OfflineException
import app.pley.android.screens.admin.dashboard.DashboardStale
import app.pley.android.ui.phone.theme.PhoneTheme
import java.io.IOException
import java.time.LocalDateTime
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The dashboard's Now playing, as the web's: where a session plays and
 * what plays it, whether it plays, its time moving on while it plays, and
 * Stop's message offered only where the player shows one.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w412dp-h915dp-xxhdpi")
class DashboardCardsUiTest {
    @get:Rule
    val compose = createComposeRule()

    private val film = ItemSummary(id = 9, libraryId = 3, kind = ItemKind.Movie, title = "A film", durationMs = 600_000)

    private fun session(player: String?, state: String = "playing", ageMs: Long? = 2_000) = DashboardSession(
        sessionId = "s1",
        username = "bea",
        device = "Living room TV",
        player = player,
        item = film,
        positionMs = 60_000,
        state = state,
        ageMs = ageMs,
    )

    @Test
    fun aPlayingSessionSaysWhatPlaysItAndItsTimeMovesOn() {
        var clock = 10_000L
        compose.mainClock.autoAdvance = false
        compose.setContent {
            PhoneTheme {
                SessionCard(session("cast"), receivedAtMs = 10_000, origin = "", onOpen = {}, onStop = {}, clockMs = { clock })
            }
        }
        compose.mainClock.advanceTimeByFrame()
        compose.onNodeWithText("Living room TV · Google Cast").assertExists()
        compose.onNodeWithTag(STATE_ICON).assertContentDescriptionEquals("Playing")
        // Reported 2 s before the list came.
        compose.onNodeWithText("bea · 1:02 / 10:00").assertExists()
        clock = 13_000L
        compose.mainClock.advanceTimeBy(1_100)
        compose.onNodeWithText("bea · 1:05 / 10:00").assertExists()
    }

    @Test
    fun aPausedSessionStaysWhereItIs() {
        var clock = 10_000L
        compose.mainClock.autoAdvance = false
        compose.setContent {
            PhoneTheme {
                SessionCard(session("jellyfin", state = "paused"), receivedAtMs = 10_000, origin = "", onOpen = {}, onStop = {}, clockMs = { clock })
            }
        }
        compose.mainClock.advanceTimeByFrame()
        compose.onNodeWithText("Living room TV · Jellyfin API").assertExists()
        compose.onNodeWithTag(STATE_ICON).assertContentDescriptionEquals("Paused")
        clock = 20_000L
        compose.mainClock.advanceTimeBy(2_000)
        compose.onNodeWithText("bea · 1:00 / 10:00").assertExists()
    }

    @Test
    fun aBufferingSessionSaysSoAndStaysWhereItIs() {
        var clock = 10_000L
        compose.mainClock.autoAdvance = false
        compose.setContent {
            PhoneTheme {
                SessionCard(session("plex", state = "buffering"), receivedAtMs = 10_000, origin = "", onOpen = {}, onStop = {}, clockMs = { clock })
            }
        }
        compose.mainClock.advanceTimeByFrame()
        compose.onNodeWithText("Living room TV · Plex API").assertExists()
        compose.onNodeWithTag(STATE_ICON).assertContentDescriptionEquals("Buffering")
        clock = 20_000L
        compose.mainClock.advanceTimeBy(2_000)
        compose.onNodeWithText("bea · 1:00 / 10:00").assertExists()
    }

    @Test
    fun aPlayingSessionStaysWhereTheListHadItWhileTheListIsntUpdating() {
        var clock = 10_000L
        compose.mainClock.autoAdvance = false
        compose.setContent {
            PhoneTheme {
                SessionCard(session("cast"), receivedAtMs = 10_000, origin = "", onOpen = {}, onStop = {}, clockMs = { clock }, live = false)
            }
        }
        clock = 40_000L
        compose.mainClock.advanceTimeBy(2_000)
        compose.onNodeWithText("bea · 1:02 / 10:00").assertExists()
    }

    @Test
    fun numbersThatStoppedUpdatingSaySinceWhenAndWhy() {
        val since = LocalDateTime.of(2026, 11, 15, 14, 32).atZone(ZoneId.systemDefault()).toEpochSecond()
        compose.setContent {
            PhoneTheme { StaleBanner(DashboardStale(since, OfflineException(IOException("down")))) }
        }
        // The time as the language shown writes it ("2:32 PM").
        compose.onNodeWithText("Not updating: these numbers are from 2:32", substring = true).assertExists()
        compose.onNodeWithText("They update again once the server answers.", substring = true).assertExists()
    }

    @Test
    fun aServerBeforePlayersSaysOnlyWhereItPlays() {
        var clock = 0L
        compose.mainClock.autoAdvance = false
        compose.setContent {
            PhoneTheme { SessionCard(session(null, ageMs = null), receivedAtMs = 0, origin = "", onOpen = {}, onStop = {}, clockMs = { clock }) }
        }
        compose.mainClock.advanceTimeByFrame()
        compose.onNodeWithText("Living room TV").assertExists()
        // Nor how long ago its place was reported: it stays there.
        clock = 5_000L
        compose.mainClock.advanceTimeBy(2_000)
        compose.onNodeWithText("bea · 1:00 / 10:00").assertExists()
    }

    @Test
    fun stopOffersAMessageForPleysOwnApps() {
        val sent = mutableListOf<String>()
        compose.setContent { PhoneTheme { StopDialog(session("pley"), onStop = { sent += it }, onDismiss = {}) } }
        compose.onNodeWithTag(STOP_MESSAGE).performTextReplacement("Bedtime")
        compose.onNodeWithTag(STOP_CONFIRM).performClick()
        assertEquals(listOf("Bedtime"), sent)
    }

    @Test
    fun stopOnlyStopsWhereThePlayerShowsNoMessage() {
        val sent = mutableListOf<String>()
        compose.setContent { PhoneTheme { StopDialog(session("airplay"), onStop = { sent += it }, onDismiss = {}) } }
        compose.onNodeWithText("A film stops for bea right away.").assertExists()
        compose.onNodeWithTag(STOP_MESSAGE).assertDoesNotExist()
        compose.onNodeWithTag(STOP_CONFIRM).performClick()
        assertEquals(listOf(""), sent)
    }
}
