package app.pley.android.ui.phone.remote

import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertHeightIsAtLeast
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import app.pley.android.data.RemoteRepository
import app.pley.android.data.RemoteSession
import app.pley.android.model.Control
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.PlayerCommandRequest
import app.pley.android.model.PlayingChannel
import app.pley.android.model.PlayingKind
import app.pley.android.model.PlayingSession
import app.pley.android.model.ProgressState
import app.pley.android.network.ApiException
import app.pley.android.ui.phone.theme.PhoneTheme
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The phone's remote: the bar above the tabs while another device plays,
 * the sheet it opens, the controls each player declares, and what it says
 * when a player doesn't answer or the server refuses.
 */
@RunWith(RobolectricTestRunner::class)
class RemoteUiTest {
    @get:Rule
    val compose = createComposeRule()

    private val remote = FakeRemote()

    private fun show() {
        compose.setContent {
            PhoneTheme {
                val sessions by remote.sessions.collectAsState()
                RemoteSlot(sessions, remote, ORIGIN)
            }
        }
        compose.waitForIdle()
    }

    private fun open() {
        compose.onNodeWithTag(REMOTE_BAR).performClick()
        compose.waitForIdle()
    }

    @Test
    fun nothingShowsWhileNoOtherDevicePlays() {
        show()
        compose.onNodeWithTag(REMOTE_BAR).assertDoesNotExist()
    }

    @Test
    fun theBarSaysWhereAndWhatPlays() {
        remote.sessions.value = listOf(episode())
        show()
        compose.onNodeWithText("Playing on Living room TV").assertExists()
        compose.onNodeWithText("Severance — S1 · E2 — Half Loop").assertExists()
        compose.onNodeWithTag(REMOTE_BAR).assertHeightIsAtLeast(48.dp)
    }

    @Test
    fun withSeveralDevicesItSaysHowManyAndTheSheetListsThemAll() {
        remote.sessions.value = listOf(episode(), channel(), episode(id = "s3", device = null))
        show()
        compose.onNodeWithText("Playing on 3 devices").assertExists()
        compose.onNodeWithText("Living room TV, Kitchen phone, Unknown device").assertExists()
        open()
        compose.onNodeWithTag(REMOTE_SHEET).assertExists()
        compose.onNodeWithTag(REMOTE_CARD + "s1").assertExists()
        compose.onNodeWithTag(REMOTE_CARD + "s2").assertExists()
        compose.onNodeWithTag(REMOTE_CARD + "s3").assertExists()
        compose.onNodeWithText("Pley News HD").assertExists()
        compose.onNodeWithText("The Evening News").assertExists()
    }

    @Test
    fun devicesOfTheSameNameAreNumbered() {
        remote.sessions.value = listOf(episode(), episode(id = "s3"))
        show()
        open()
        compose.onNodeWithText("Living room TV (1)").assertExists()
        compose.onNodeWithContentDescription("Pause on Living room TV (2)").assertExists()
    }

    @Test
    fun onlyTheControlsThePlayerDeclaresShowStopAlways() {
        remote.sessions.value = listOf(episode(), channel())
        show()
        open()
        listOf(
            "Back 10 seconds on Living room TV",
            "Pause on Living room TV",
            "Forward 30 seconds on Living room TV",
            "Next on Living room TV",
            "Seek on Living room TV",
            "Stop on Living room TV",
            "Previous channel on Kitchen phone",
            "Next channel on Kitchen phone",
            "Stop on Kitchen phone",
        ).forEach { label ->
            compose.onNodeWithContentDescription(label).assertExists()
            assertTouchTarget(label)
        }
        // The episode's player has no previous, the channel no pause and no position.
        compose.onNodeWithContentDescription("Previous on Living room TV").assertDoesNotExist()
        compose.onAllNodesWithContentDescription("Pause on Kitchen phone").assertCountEquals(0)
        compose.onNodeWithContentDescription("Seek on Kitchen phone").assertDoesNotExist()
    }

    /** [label]'s control takes touches over 48 by 48 dp at least (the slider's track is drawn smaller). */
    private fun assertTouchTarget(label: String) {
        val node = compose.onNodeWithContentDescription(label).fetchSemanticsNode()
        val density = node.layoutInfo.density.density
        val bounds = node.touchBoundsInRoot
        assertTrue("$label: ${bounds.height / density} dp high", bounds.height / density >= 47.5f)
        assertTrue("$label: ${bounds.width / density} dp wide", bounds.width / density >= 47.5f)
    }

    @Test
    fun aPausedPlayerOffersPlayAndAPlayerWithoutSeekShowsItsProgress() {
        remote.sessions.value = listOf(episode(state = ProgressState.Paused, controls = listOf(Control.Play, Control.Pause)))
        show()
        open()
        compose.onNodeWithContentDescription("Play on Living room TV").assertExists()
        // The times beside the bar aren't read again: the bar says them.
        compose.onNodeWithContentDescription("Progress on Living room TV").assert(saying("19:45 of 53:29"))
        compose.onNodeWithText("19:45").assertDoesNotExist()
    }

    @Test
    fun whilePlayingThePositionIsEstimatedEachSecond() {
        remote.sessions.value = listOf(episode())
        show()
        open()
        // Reported at 19:45, 4.2 s before the answer.
        compose.onNodeWithContentDescription("Seek on Living room TV").assert(saying("19:49 of 53:29"))
        remote.now += 3_000
        compose.mainClock.advanceTimeBy(1_000)
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Seek on Living room TV").assert(saying("19:52 of 53:29"))
    }

    private fun saying(state: String) = SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, state)

    @Test
    fun aCommandGoesToItsSessionAndTheListShowingTheChangeAnswersIt() {
        remote.sessions.value = listOf(episode())
        show()
        open()
        compose.onNodeWithContentDescription("Pause on Living room TV").performClick()
        compose.waitForIdle()
        assertEquals(listOf("s1" to PlayerCommandRequest(Control.Pause)), remote.commands)
        compose.onNodeWithContentDescription("Forward 30 seconds on Living room TV").performClick()
        compose.waitForIdle()
        assertEquals("s1" to PlayerCommandRequest(Control.Skip, offsetMs = 30_000), remote.commands.last())

        // The player paused and reported.
        remote.sessions.value = listOf(episode(state = ProgressState.Paused))
        compose.mainClock.advanceTimeBy(RemoteRepository.ANSWER_WAIT_MS + 1_000)
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Play on Living room TV").assertExists()
        compose.onNodeWithText("Living room TV didn't answer.").assertDoesNotExist()
    }

    @Test
    fun aPlayerThatShowsNoChangeWithinFiveSecondsDidNotAnswer() {
        remote.sessions.value = listOf(episode())
        show()
        open()
        compose.onNodeWithContentDescription("Pause on Living room TV").performClick()
        compose.waitForIdle()
        compose.mainClock.advanceTimeBy(RemoteRepository.ANSWER_WAIT_MS - 1_000)
        compose.waitForIdle()
        compose.onNodeWithText("Waiting for Living room TV").assertExists()
        compose.onNodeWithText("Living room TV didn't answer.").assertDoesNotExist()
        compose.mainClock.advanceTimeBy(2_000)
        compose.waitForIdle()
        // Asked once more before saying so.
        assertEquals(1, remote.refreshes)
        compose.onNodeWithText("Living room TV didn't answer.").assertExists()
        compose.onNodeWithText("Waiting for Living room TV").assertDoesNotExist()

        // It acted late after all: its next report takes the message away.
        val late = episode(state = ProgressState.Paused)
        remote.sessions.value = listOf(late.copy(session = late.session.copy(updatedAt = late.session.updatedAt + 8)))
        compose.waitForIdle()
        compose.onNodeWithText("Living room TV didn't answer.").assertDoesNotExist()
    }

    @Test
    fun aNextThatChangesNothingIsNoFailure() {
        remote.sessions.value = listOf(episode())
        show()
        open()
        compose.onNodeWithContentDescription("Next on Living room TV").performClick()
        compose.mainClock.advanceTimeBy(RemoteRepository.ANSWER_WAIT_MS + 1_000)
        compose.waitForIdle()
        compose.onNodeWithText("Living room TV didn't answer.").assertDoesNotExist()
    }

    @Test
    fun refusalsAreSaidPlainly() {
        remote.sessions.value = listOf(episode())
        show()
        open()
        remote.failure = ApiException(409, "not_controllable", "The player doesn't do that")
        compose.onNodeWithContentDescription("Pause on Living room TV").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Living room TV can't do that.").assertExists()

        remote.failure = ApiException(404, "not_found", "No such session")
        compose.onNodeWithContentDescription("Pause on Living room TV").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Living room TV isn't playing any more.").assertExists()
        compose.onAllNodesWithText("Living room TV can't do that.").assertCountEquals(0)

        remote.failure = ApiException(429, "too_many_requests", "Slow down")
        compose.onNodeWithContentDescription("Pause on Living room TV").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Too many at once: try again in a moment.").assertExists()
    }

    @Test
    fun stopStopsTheSessionAndTheSheetClosesWithNothingLeft() {
        remote.sessions.value = listOf(episode())
        show()
        open()
        compose.onNodeWithContentDescription("Stop on Living room TV").performClick()
        compose.waitForIdle()
        assertEquals(listOf("s1"), remote.stopped)
        remote.sessions.value = emptyList()
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_SHEET).assertDoesNotExist()
        compose.onNodeWithTag(REMOTE_BAR).assertDoesNotExist()
    }

    /** The remote as the test drives it: its sessions, its clock, and what it was asked. */
    private class FakeRemote : RemoteRepository {
        override val sessions = MutableStateFlow<List<RemoteSession>>(emptyList())
        var now = 10_000L
        override val clock: () -> Long = { now }
        val commands = mutableListOf<Pair<String, PlayerCommandRequest>>()
        val stopped = mutableListOf<String>()
        var refreshes = 0
        var failure: Exception? = null

        override suspend fun refresh(): List<RemoteSession> {
            refreshes++
            return sessions.value
        }

        override suspend fun command(sessionId: String, command: PlayerCommandRequest) {
            failure?.let { throw it }
            commands += sessionId to command
        }

        override suspend fun stop(sessionId: String) {
            stopped += sessionId
        }

        override fun here(player: Any, sessionId: String?) = Unit
    }

    private companion object {
        const val ORIGIN = "http://127.0.0.1:9"

        /** Severance S1 · E2 on the Living room TV, at 19:45 of 53:29, reported 4.2 s before the answer at 10 s. */
        fun episode(
            id: String = "s1",
            device: String? = "Living room TV",
            state: ProgressState = ProgressState.Playing,
            controls: List<Control> = listOf(Control.Play, Control.Pause, Control.Seek, Control.Skip, Control.Next),
        ) = RemoteSession(
            PlayingSession(
                sessionId = id,
                kind = PlayingKind.Item,
                device = device,
                item = ItemSummary(
                    204, 2, ItemKind.Episode, "Half Loop", index = 2, parentIndex = 1, parentId = 202, grandparentId = 201,
                    grandparentTitle = "Severance",
                ),
                state = state,
                positionMs = 1_185_000,
                durationMs = 3_209_000,
                updatedAt = 1_761_390_000,
                ageMs = 4_200,
                controls = controls,
            ),
            reportedAt = 5_800,
        )

        /** A channel on the Kitchen phone, which goes up and down. */
        fun channel() = RemoteSession(
            PlayingSession(
                sessionId = "s2",
                kind = PlayingKind.Live,
                device = "Kitchen phone",
                channel = PlayingChannel(1, "Pley News HD", number = "101", now = "The Evening News"),
                state = ProgressState.Playing,
                controls = listOf(Control.Next, Control.Previous),
            ),
            reportedAt = 9_100,
        )
    }
}
