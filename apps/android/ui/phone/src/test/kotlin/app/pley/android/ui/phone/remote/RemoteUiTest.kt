package app.pley.android.ui.phone.remote

import android.text.format.DateFormat
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertHasNoClickAction
import androidx.compose.ui.test.assertHeightIsAtLeast
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.unit.dp
import app.pley.android.data.OtherDevice
import app.pley.android.data.RemoteRepository
import app.pley.android.data.RemoteSession
import app.pley.android.model.CastKind
import app.pley.android.model.CastRequest
import app.pley.android.model.Control
import app.pley.android.model.DeviceKind
import app.pley.android.model.DeviceProfileName
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.PlayerCommandRequest
import app.pley.android.model.PlayingChannel
import app.pley.android.model.PlayingKind
import app.pley.android.model.PlayingSession
import app.pley.android.model.ProgressState
import app.pley.android.model.QualityOption
import app.pley.android.model.RemoteDevice
import app.pley.android.model.SessionTracks
import app.pley.android.model.TrackOption
import app.pley.android.network.ApiException
import app.pley.android.ui.phone.theme.PhoneTheme
import java.util.Date
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment

/**
 * The phone's remote: Home's button while another device is on, the sheet
 * it opens with "Your devices", the bar above the tabs only while this
 * phone controls what a device plays, a device's controls (those its
 * player declares, Next where what plays has one, its audio, subtitles and
 * quality, a channel's programme), Stop that asks first, devices ready to
 * play that are only listed, commands that show at once, and what it says
 * when a player doesn't answer or the server refuses.
 */
@RunWith(RobolectricTestRunner::class)
class RemoteUiTest {
    @get:Rule
    val compose = createComposeRule()

    private val remote = FakeRemote()

    /** The remote as the shell has it: Home's button, which asks for the sheet, and the slot above the tabs. */
    private fun show(asked: RemoteAsk? = null) {
        compose.setContent {
            PhoneTheme {
                val devices by remote.devices.collectAsState()
                var ask by remember { mutableStateOf(asked) }
                Column {
                    RemoteButton(devices, onOpen = { ask = RemoteAsk() })
                    RemoteSlot(devices, remote, ORIGIN, ask)
                }
            }
        }
        compose.waitForIdle()
    }

    private fun open() {
        compose.onNodeWithTag(REMOTE_BUTTON).performClick()
        compose.waitForIdle()
    }

    /** Opens the sheet and the controls of [deviceId]. */
    private fun control(deviceId: String) {
        open()
        compose.onNodeWithTag(REMOTE_DEVICE + deviceId).performClick()
        compose.waitForIdle()
    }

    @Test
    fun nothingShowsWhileNoOtherDeviceIsOn() {
        show()
        compose.onNodeWithTag(REMOTE_BUTTON).assertDoesNotExist()
        compose.onNodeWithTag(REMOTE_BAR).assertDoesNotExist()
    }

    @Test
    fun theButtonSaysHowManyPlayAndTheBarWaitsForACommandFromHere() {
        remote.devices.value = listOf(episode(), bedroom())
        show()
        compose.onNodeWithContentDescription("Remote, 1 device playing").assertExists()
        // Playing elsewhere, not controlled from here: no bar.
        compose.onNodeWithTag(REMOTE_BAR).assertDoesNotExist()
        control("app-12")
        compose.onNodeWithContentDescription("Pause on Living room TV").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Playing on Living room TV").assertExists()
        compose.onNodeWithText("A Show — S1 · E2 — The Second One").assertExists()
        compose.onNodeWithTag(REMOTE_BAR).assertHeightIsAtLeast(48.dp)
        // The player paused and reported.
        remote.devices.value = listOf(episode(state = ProgressState.Paused), bedroom())
        compose.waitForIdle()
        compose.onNodeWithText("Paused on Living room TV").assertExists()
    }

    @Test
    fun theBarOpensTheControlsOfTheDeviceItControlsAndGoesWhenItPlaysNothing() {
        remote.devices.value = listOf(episode(), channel())
        show()
        control("app-15")
        compose.onNodeWithContentDescription("Next channel on Kitchen phone").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Playing on Kitchen phone").assertExists()
        // Back at the list, the bar's action opens the device's controls again. (A person closes the sheet first:
        // its scrim covers the bar. The test calls the bar's action, which a tap or TalkBack runs.)
        compose.onNodeWithContentDescription("Back to your devices").performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_CONTROLS + "app-15").assertDoesNotExist()
        compose.onNodeWithTag(REMOTE_BAR).performSemanticsAction(SemanticsActions.OnClick)
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_CONTROLS + "app-15").assertExists()

        // Stopped there, or ended: the bar goes after a moment, and a later title there isn't this phone's.
        // (A next keeps the session it ended while a command's answer is waited for; the phone lets go as long after.)
        remote.devices.value = listOf(episode())
        compose.mainClock.advanceTimeBy(2 * RemoteRepository.ANSWER_WAIT_MS + 1_000)
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_BAR).assertDoesNotExist()
        remote.devices.value = listOf(episode(), channel())
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_BAR).assertDoesNotExist()
    }

    @Test
    fun devicesOnButPlayingNothingShowOnlyTheButton() {
        remote.devices.value = listOf(bedroom())
        show()
        compose.onNodeWithContentDescription("Remote, 1 device ready to play").assertExists()
        compose.onNodeWithTag(REMOTE_BAR).assertDoesNotExist()
        open()
        compose.onNodeWithText("Your devices").assertExists()
        compose.onNodeWithText("Bedroom TV").assertExists()
    }

    @Test
    fun theListHasARowForEachDeviceInTheOrderGivenWhatEachPlaysOrThatItIsReady() {
        remote.devices.value = listOf(episode(), channel(), bedroom(), episode(id = "s3", deviceId = "token-4", device = null, receiver = false))
        show()
        compose.onNodeWithContentDescription("Remote, 3 devices playing").assertExists()
        open()
        compose.onNodeWithText("Your devices").assertExists()
        val tops = listOf("app-12", "app-15", "app-20", "token-4").map { id ->
            compose.onNodeWithTag(REMOTE_DEVICE + id).fetchSemanticsNode().positionInRoot.y
        }
        assertEquals(tops.sorted(), tops)
        compose.onNodeWithText("101 Pley News HD").assertExists()
        compose.onNodeWithText("Ready to play").assertExists()
        assertEquals(2, compose.onAllNodesWithText("A Show — S1 · E2 — The Second One").fetchSemanticsNodes().size)
    }

    @Test
    fun withOneDeviceOnlyItsControlsShowAtOnce() {
        remote.devices.value = listOf(episode())
        show()
        open()
        compose.onNodeWithTag(REMOTE_CONTROLS + "app-12").assertExists()
        compose.onNodeWithText("A Show · S1 · E2").assertExists()
        compose.onNodeWithText("The Second One").assertExists()
        // Nowhere to go back to.
        compose.onNodeWithContentDescription("Back to your devices").assertDoesNotExist()
    }

    @Test
    fun choosingADeviceShowsItsControlsInTheListsPlaceAndBackReturnsToIt() {
        remote.devices.value = listOf(episode(), channel())
        show()
        control("app-15")
        compose.onNodeWithTag(REMOTE_CONTROLS + "app-15").assertExists()
        compose.onNodeWithTag(REMOTE_DEVICE + "app-12").assertDoesNotExist()
        compose.onNodeWithContentDescription("Back to your devices").performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_DEVICE + "app-12").assertExists()
        compose.onNodeWithTag(REMOTE_CONTROLS + "app-15").assertDoesNotExist()
    }

    @Test
    fun aDeviceReadyToPlayIsListedWithNothingToChoose() {
        remote.devices.value = listOf(episode(), bedroom())
        show()
        open()
        compose.onNodeWithTag(REMOTE_DEVICE + "app-20").assertHasNoClickAction()
        compose.onNodeWithTag(REMOTE_DEVICE + "app-12").assertHasClickAction()
        compose.onNodeWithText("Ready to play").assertExists()

        // Alone, it is listed too, with no controls.
        remote.devices.value = listOf(bedroom())
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_DEVICE + "app-20").assertHasNoClickAction()
        compose.onNodeWithTag(REMOTE_CONTROLS + "app-20").assertDoesNotExist()
    }

    @Test
    fun anotherProfilesDeviceSaysWhoseItIsAndNothingOfWhatItPlays() {
        val sams = OtherDevice(
            "app-31",
            receiver = RemoteDevice("tv-3", "app-31", "Den TV", DeviceKind.Tv, listOf(CastKind.Item), profile = DeviceProfileName(3, "Sam")),
        )
        remote.devices.value = listOf(episode(), bedroom(), sams)
        show()
        // Ready to play, with the profile: the button counts it with the others.
        compose.onNodeWithContentDescription("Remote, 1 device playing").assertExists()
        open()
        compose.onNodeWithText("Ready to play · on Sam's profile").assertExists()
        compose.onNodeWithTag(REMOTE_DEVICE + "app-31").assertHasNoClickAction()
        compose.onNodeWithText("Ready to play").assertExists()
    }

    @Test
    fun theDeviceAskedForShowsItsControlsOnceItPlays() {
        remote.devices.value = listOf(episode(), bedroom())
        // Play on… sent it something.
        show(asked = RemoteAsk("app-20"))
        compose.onNodeWithTag(REMOTE_DEVICE + "app-20").assertExists()
        compose.onNodeWithTag(REMOTE_CONTROLS + "app-20").assertDoesNotExist()
        val bedroom = episode(id = "s9", deviceId = "app-20", device = "Bedroom TV")
        remote.devices.value = listOf(episode(), bedroom)
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_CARD + "s9").assertExists()
        compose.onNodeWithContentDescription("Pause on Bedroom TV").assertExists()
        // Sent from here: the bar follows it.
        compose.onNodeWithText("Playing on Bedroom TV").assertExists()
    }

    @Test
    fun aCastToADevicePlayingSomethingElseShowsTheBarOnceWhatWasSentPlays() {
        remote.devices.value = listOf(episode())
        show(asked = RemoteAsk("app-12"))
        // What played there before isn't this phone's.
        compose.onNodeWithTag(REMOTE_BAR).assertDoesNotExist()
        // The old title stops before the new one is listed.
        remote.devices.value = listOf(episode().copy(session = null))
        compose.mainClock.advanceTimeBy(1_000)
        compose.waitForIdle()
        remote.devices.value = listOf(episode(id = "s9"))
        compose.waitForIdle()
        compose.onNodeWithText("Playing on Living room TV").assertExists()
    }

    @Test
    fun aTitleEndingIntoTheNextKeepsTheBar() {
        remote.devices.value = listOf(episode())
        show()
        open()
        compose.onNodeWithContentDescription("Pause on Living room TV").performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_BAR).assertExists()
        // The device's own Up Next: a moment playing nothing, then the next episode.
        remote.devices.value = listOf(episode().copy(session = null))
        compose.mainClock.advanceTimeBy(2_000)
        compose.waitForIdle()
        remote.devices.value = listOf(episode(id = "s5"))
        compose.mainClock.advanceTimeBy(RemoteRepository.ANSWER_WAIT_MS)
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_BAR).assertExists()
    }

    @Test
    fun aCastThatNeverStartedIsNotWaitedForForever() {
        remote.devices.value = listOf(episode(), bedroom())
        show(asked = RemoteAsk("app-20"))
        compose.mainClock.advanceTimeBy(CAST_START_WAIT_MS + 1_000)
        compose.waitForIdle()
        // Played later, by someone else: not this phone's.
        remote.devices.value = listOf(episode(), episode(id = "s9", deviceId = "app-20", device = "Bedroom TV"))
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_BAR).assertDoesNotExist()
    }

    @Test
    fun devicesOfTheSameNameAreNumbered() {
        remote.devices.value = listOf(episode(), episode(id = "s3", deviceId = "app-13"))
        show()
        open()
        compose.onNodeWithText("Living room TV (1)").assertExists()
        compose.onNodeWithTag(REMOTE_DEVICE + "app-13").performClick()
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Pause on Living room TV (2)").assertExists()
    }

    @Test
    fun onlyTheControlsThePlayerDeclaresShowNextAndStopAlways() {
        remote.devices.value = listOf(episode(), channel())
        show()
        control("app-12")
        listOf(
            "Back 10 seconds on Living room TV",
            "Pause on Living room TV",
            "Forward 30 seconds on Living room TV",
            "Next on Living room TV",
            "Seek on Living room TV",
            "Audio on Living room TV",
            "Subtitles on Living room TV",
            "Quality on Living room TV",
            "Stop on Living room TV",
        ).forEach { label ->
            compose.onNodeWithContentDescription(label).assertExists()
            assertTouchTarget(label)
        }
        // The episode's player has no previous.
        compose.onNodeWithContentDescription("Previous on Living room TV").assertDoesNotExist()

        compose.onNodeWithContentDescription("Back to your devices").performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_DEVICE + "app-15").performClick()
        compose.waitForIdle()
        listOf("Previous channel on Kitchen phone", "Next channel on Kitchen phone", "Stop on Kitchen phone").forEach { label ->
            compose.onNodeWithContentDescription(label).assertExists()
            assertTouchTarget(label)
        }
        compose.onNodeWithContentDescription("Next channel on Kitchen phone").assertIsEnabled()
        compose.onNodeWithContentDescription("Previous channel on Kitchen phone").assertIsEnabled()
        // Both in one rocker, the word between them drawn only.
        compose.onNodeWithTag(REMOTE_ROCKER).assertExists()
        compose.onNodeWithText("CHANNEL", useUnmergedTree = true).assertDoesNotExist()
        // A channel has no pause, no skips, no place, no tracks and no quality.
        compose.onAllNodesWithContentDescription("Pause on Kitchen phone").assertCountEquals(0)
        compose.onNodeWithContentDescription("Back 10 seconds on Kitchen phone").assertDoesNotExist()
        compose.onNodeWithContentDescription("Seek on Kitchen phone").assertDoesNotExist()
        compose.onNodeWithContentDescription("Audio on Kitchen phone").assertDoesNotExist()
        compose.onNodeWithContentDescription("Quality on Kitchen phone").assertDoesNotExist()
        compose.onNodeWithText("The Evening News").assertExists()
    }

    @Test
    fun previousAndNextSitBesideTheSkipsAndPlayInOneCentredRow() {
        remote.devices.value = listOf(episode(controls = listOf(Control.Previous, Control.Next, Control.Play, Control.Pause, Control.Skip)))
        show()
        open()
        val card = compose.onNodeWithTag(REMOTE_CARD + "s1").fetchSemanticsNode()
        val density = card.layoutInfo.density.density
        fun bounds(label: String) = compose.onNodeWithContentDescription(label).fetchSemanticsNode().boundsInRoot
        val previous = bounds("Previous on Living room TV")
        val back = bounds("Back 10 seconds on Living room TV")
        val pause = bounds("Pause on Living room TV")
        val forward = bounds("Forward 30 seconds on Living room TV")
        val next = bounds("Next on Living room TV")
        assertTrue(previous.right <= back.left && back.right <= pause.left && pause.right <= forward.left && forward.right <= next.left)
        assertEquals(card.boundsInRoot.center.x, pause.center.x, density)
        // Side by side, not spread to the card's edges.
        assertEquals(back.left, previous.right, density)
        assertEquals(next.left, forward.right, density)

        // Without a Previous, its place is kept: play stays in the middle.
        remote.devices.value = listOf(episode())
        compose.waitForIdle()
        assertEquals(card.boundsInRoot.center.x, bounds("Pause on Living room TV").center.x, density)
    }

    @Test
    fun aChannelShowsItsProgrammeHowFarItIsItsTimesAndWhatIsNext() {
        val start = System.currentTimeMillis() / 1000 - 1_800
        remote.devices.value = listOf(channel(nowStart = start, nowStop = start + 3_600, next = "The Late Show"))
        show()
        open()
        // The channel over Live TV, its programme under it with its times as the device writes them.
        compose.onNodeWithText("101 Pley News HD").assertExists()
        compose.onNodeWithText("Live TV").assertExists()
        compose.onNodeWithText("The Evening News").assertExists()
        val format = DateFormat.getTimeFormat(RuntimeEnvironment.getApplication())
        val span = format.format(Date(start * 1000)) + " – " + format.format(Date((start + 3_600) * 1000))
        compose.onNodeWithText(span).assertExists()
        compose.onNodeWithText("Next: The Late Show").assertExists()
        // Its player pauses: Pause shows between the channel buttons.
        remote.devices.value = listOf(channel(controls = listOf(Control.Next, Control.Previous, Control.Pause)))
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Pause on Kitchen phone").assertExists()
        // Without a guide, nothing but the channel.
        remote.devices.value = listOf(channel(now = null))
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_LIVE).assertDoesNotExist()
    }

    @Test
    fun aChannelWhosePlayerDoesNotChangeChannelHasItsButtonsDimmed() {
        remote.devices.value = listOf(channel(controls = emptyList()))
        show()
        open()
        compose.onNodeWithContentDescription("Previous channel on Kitchen phone").assertIsNotEnabled()
        compose.onNodeWithContentDescription("Next channel on Kitchen phone").assertIsNotEnabled()
    }

    @Test
    fun anEpisodesNextShowsDimmedWhenThePlayerDidNotDeclareIt() {
        remote.devices.value = listOf(episode(controls = listOf(Control.Play, Control.Pause)))
        show()
        open()
        compose.onNodeWithContentDescription("Next on Living room TV").assertExists().assertIsNotEnabled()
        // Nor audio, subtitles and quality: not declared, not offered, whatever the file has.
        compose.onNodeWithTag(REMOTE_AUDIO).assertDoesNotExist()
        compose.onNodeWithTag(REMOTE_SUBTITLES).assertDoesNotExist()
        compose.onNodeWithTag(REMOTE_QUALITY).assertDoesNotExist()
    }

    @Test
    fun aMovieWhosePlayerDeclaresNoNextHasNone() {
        remote.devices.value = listOf(movie(controls = listOf(Control.Play, Control.Pause, Control.Seek)))
        show()
        open()
        compose.onNodeWithContentDescription("Pause on Living room TV").assertExists()
        compose.onNodeWithContentDescription("Next on Living room TV").assertDoesNotExist()
        // Declared (a movie in a playlist goes on to the next entry), it shows.
        remote.devices.value = listOf(movie(controls = listOf(Control.Play, Control.Pause, Control.Next)))
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Next on Living room TV").assertIsEnabled()
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
    fun theTrackMenusListTheTracksAsThePlayerLabelsThemTheOnePlayingTicked() {
        remote.devices.value = listOf(episode())
        show()
        open()
        compose.onNodeWithContentDescription("Audio on Living room TV").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("English (EAC3 5.1)").assertIsSelected()
        compose.onNodeWithText("French (AAC Stereo)").assertIsNotSelected().performClick()
        compose.waitForIdle()
        assertEquals("s1" to PlayerCommandRequest(Control.Audio, streamIndex = 2), remote.commands.last())

        compose.onNodeWithContentDescription("Subtitles on Living room TV").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Off").assertIsSelected()
        compose.onNodeWithText("English (SRT)").assertExists()
        compose.onNodeWithText("French (SRT, Forced)").performClick()
        compose.waitForIdle()
        assertEquals("s1" to PlayerCommandRequest(Control.Subtitles, streamIndex = 4), remote.commands.last())
    }

    @Test
    fun theQualityMenuListsWhatThePlayersMenuOffersItsPickTicked() {
        remote.devices.value = listOf(episode())
        show()
        open()
        compose.onNodeWithTag(REMOTE_QUALITY).assertIsNotSelected().performClick()
        compose.waitForIdle()
        // Selected while its menu is open.
        compose.onNodeWithTag(REMOTE_QUALITY).assertIsSelected()
        compose.onNodeWithText("Automatic").assertIsSelected()
        compose.onNodeWithText("Original").assertIsNotSelected()
        compose.onNodeWithText("4 Mbps 720p").performClick()
        compose.waitForIdle()
        assertEquals("s1" to PlayerCommandRequest(Control.Quality, quality = "4m"), remote.commands.last())
        compose.onNodeWithTag(REMOTE_QUALITY).assertIsNotSelected()
        // Ticked at once.
        compose.onNodeWithTag(REMOTE_QUALITY).performClick()
        compose.waitForIdle()
        compose.onNodeWithText("4 Mbps 720p").assertIsSelected()
        // Answered by the player saying it picked it.
        remote.devices.value = listOf(episode(quality = "4m"))
        compose.mainClock.advanceTimeBy(RemoteRepository.ANSWER_WAIT_MS + 1_000)
        compose.waitForIdle()
        compose.onNodeWithText("Living room TV didn't answer.").assertDoesNotExist()
    }

    @Test
    fun noQualityIsTickedUntilThePlayerSaysWhich() {
        remote.devices.value = listOf(episode(quality = null))
        show()
        open()
        compose.onNodeWithContentDescription("Quality on Living room TV").performClick()
        compose.waitForIdle()
        listOf("Original", "Automatic", "8 Mbps 1080p", "4 Mbps 720p").forEach { compose.onNodeWithText(it).assertIsNotSelected() }
    }

    @Test
    fun audioSubtitlesAndQualityShowOnlyWhenThereIsAChoice() {
        val one = episode()
        val session = one.session!!.session
        val tracks = session.tracks.copy(audio = session.tracks.audio.take(1), subtitles = emptyList())
        remote.devices.value = listOf(one.copy(session = one.session!!.copy(session = session.copy(tracks = tracks, qualities = emptyList()))))
        show()
        open()
        // One audio track, no subtitles, no qualities: nothing to pick; Stop stays.
        compose.onNodeWithTag(REMOTE_AUDIO).assertDoesNotExist()
        compose.onNodeWithTag(REMOTE_SUBTITLES).assertDoesNotExist()
        compose.onNodeWithTag(REMOTE_QUALITY).assertDoesNotExist()
        compose.onNodeWithContentDescription("Stop on Living room TV").assertExists()
    }

    @Test
    fun aTrackPickIsAnsweredByThePlayerSayingItPlaysIt() {
        remote.devices.value = listOf(episode())
        show()
        open()
        compose.onNodeWithContentDescription("Audio on Living room TV").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("French (AAC Stereo)").performClick()
        compose.waitForIdle()
        // Ticked at once.
        compose.onNodeWithContentDescription("Audio on Living room TV").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("French (AAC Stereo)").assertIsSelected()
        remote.devices.value = listOf(episode(audioIndex = 2))
        compose.waitForIdle()
        compose.onNodeWithText("French (AAC Stereo)").assertIsSelected()
        compose.mainClock.advanceTimeBy(RemoteRepository.ANSWER_WAIT_MS + 1_000)
        compose.waitForIdle()
        compose.onNodeWithText("Living room TV didn't answer.").assertDoesNotExist()
    }

    @Test
    fun aTrackPickThePlayerDoesNotTakeUpDidNotAnswer() {
        remote.devices.value = listOf(episode())
        show()
        open()
        compose.onNodeWithContentDescription("Subtitles on Living room TV").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("English (SRT)").performClick()
        compose.mainClock.advanceTimeBy(RemoteRepository.ANSWER_WAIT_MS + 1_000)
        compose.waitForIdle()
        compose.onNodeWithText("Living room TV didn't answer.").assertExists()
    }

    @Test
    fun aPausedPlayerOffersPlayAndAPlayerWithoutSeekShowsItsProgress() {
        remote.devices.value = listOf(episode(state = ProgressState.Paused, controls = listOf(Control.Play, Control.Pause)))
        show()
        open()
        compose.onNodeWithContentDescription("Play on Living room TV").assertExists()
        // The times beside the bar aren't read again: the bar says them.
        compose.onNodeWithContentDescription("Progress on Living room TV").assert(saying("19:45 of 53:29"))
        compose.onNodeWithText("19:45").assertDoesNotExist()
    }

    @Test
    fun whilePlayingThePositionIsEstimatedEachSecond() {
        remote.devices.value = listOf(episode())
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
        remote.devices.value = listOf(episode())
        show()
        open()
        compose.onNodeWithContentDescription("Pause on Living room TV").performClick()
        compose.waitForIdle()
        assertEquals(listOf("s1" to PlayerCommandRequest(Control.Pause)), remote.commands)
        compose.onNodeWithContentDescription("Forward 30 seconds on Living room TV").performClick()
        compose.waitForIdle()
        assertEquals("s1" to PlayerCommandRequest(Control.Skip, offsetMs = 30_000), remote.commands.last())

        // The player paused and reported.
        remote.devices.value = listOf(episode(state = ProgressState.Paused))
        compose.mainClock.advanceTimeBy(RemoteRepository.ANSWER_WAIT_MS + 1_000)
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Play on Living room TV").assertExists()
        compose.onNodeWithText("Living room TV didn't answer.").assertDoesNotExist()
    }

    @Test
    fun aPlayerThatShowsNoChangeWithinFiveSecondsDidNotAnswer() {
        remote.devices.value = listOf(episode())
        show()
        open()
        compose.onNodeWithContentDescription("Pause on Living room TV").performClick()
        compose.waitForIdle()
        // Paused at once, and nothing said while it waits.
        compose.onNodeWithContentDescription("Play on Living room TV").assertExists()
        compose.mainClock.advanceTimeBy(RemoteRepository.ANSWER_WAIT_MS - 1_000)
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Play on Living room TV").assertExists()
        compose.onNodeWithText("Living room TV didn't answer.").assertDoesNotExist()
        compose.mainClock.advanceTimeBy(2_000)
        compose.waitForIdle()
        // Asked once more before saying so, and playing again as the player says.
        assertEquals(1, remote.refreshes)
        compose.onNodeWithText("Living room TV didn't answer.").assertExists()
        compose.onNodeWithContentDescription("Pause on Living room TV").assertExists()

        // It acted late after all: its next report takes the message away.
        val late = episode(state = ProgressState.Paused)
        remote.devices.value = listOf(late.copy(session = late.session!!.copy(session = late.session!!.session.copy(updatedAt = UPDATED_AT + 8))))
        compose.waitForIdle()
        compose.onNodeWithText("Living room TV didn't answer.").assertDoesNotExist()
    }

    @Test
    fun aNextThatChangesNothingIsNoFailure() {
        remote.devices.value = listOf(episode())
        show()
        open()
        compose.onNodeWithContentDescription("Next on Living room TV").performClick()
        compose.mainClock.advanceTimeBy(RemoteRepository.ANSWER_WAIT_MS + 1_000)
        compose.waitForIdle()
        compose.onNodeWithText("Living room TV didn't answer.").assertDoesNotExist()
    }

    @Test
    fun aNextKeepsWhatPlayedUntilTheNextOnePlays() {
        // A player that takes no casts goes from the list between its sessions.
        remote.devices.value = listOf(episode(receiver = false))
        show()
        open()
        compose.onNodeWithContentDescription("Next on Living room TV").performClick()
        compose.waitForIdle()
        remote.devices.value = emptyList()
        compose.mainClock.advanceTimeBy(RemoteRepository.ANSWER_WAIT_MS - 1_000)
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_CARD + "s1").assertExists()
        compose.onNodeWithText("Playing on Living room TV").assertExists()
        remote.devices.value = listOf(episode(id = "s4", receiver = false))
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_CARD + "s4").assertExists()

        // A device that takes casts doesn't show as ready to play between them either.
        remote.devices.value = listOf(episode(id = "s4"))
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Next on Living room TV").performClick()
        compose.waitForIdle()
        remote.devices.value = listOf(episode().copy(session = null))
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_CARD + "s4").assertExists()
        compose.onNodeWithText("Ready to play").assertDoesNotExist()
    }

    @Test
    fun aSeekOrASkipShowsWhereItGoesAtOnce() {
        remote.devices.value = listOf(episode())
        show()
        open()
        val seek = compose.onNodeWithContentDescription("Seek on Living room TV")
        seek.performSemanticsAction(SemanticsActions.SetProgress) { it(0.5f) }
        compose.waitForIdle()
        assertEquals("s1" to PlayerCommandRequest(Control.Seek, positionMs = 1_604_500), remote.commands.last())
        seek.assert(saying("26:44 of 53:29"))
        // A skip goes from there, not from where the player last said it was.
        compose.onNodeWithContentDescription("Forward 30 seconds on Living room TV").performClick()
        compose.waitForIdle()
        seek.assert(saying("27:14 of 53:29"))
        // The player never went: once the wait is over it says so, and where it is shows again.
        compose.mainClock.advanceTimeBy(RemoteRepository.ANSWER_WAIT_MS + 1_000)
        compose.waitForIdle()
        compose.onNodeWithText("Living room TV didn't answer.").assertExists()
        seek.assert(saying("19:49 of 53:29"))
    }

    @Test
    fun nothingListedOnceTheAnswerWaitIsOverClosesTheSheet() {
        remote.devices.value = listOf(episode(receiver = false))
        show()
        open()
        compose.onNodeWithContentDescription("Next on Living room TV").performClick()
        compose.waitForIdle()
        remote.devices.value = emptyList()
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_SHEET).assertExists()
        compose.mainClock.advanceTimeBy(RemoteRepository.ANSWER_WAIT_MS + 1_000)
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_SHEET).assertDoesNotExist()
        compose.onNodeWithTag(REMOTE_BAR).assertDoesNotExist()
    }

    @Test
    fun refusalsAreSaidPlainly() {
        remote.devices.value = listOf(episode())
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
    fun stopAsksFirstAndCancelLeavesItPlaying() {
        remote.devices.value = listOf(episode())
        show()
        open()
        compose.onNodeWithContentDescription("Stop on Living room TV").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Stop playing on Living room TV?").assertExists()
        assertTrue(remote.stopped.isEmpty())
        compose.onNodeWithText("Cancel").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Stop playing on Living room TV?").assertDoesNotExist()
        assertTrue(remote.stopped.isEmpty())
        compose.onNodeWithContentDescription("Pause on Living room TV").assertExists()
    }

    @Test
    fun stopStopsTheSessionAndTheDeviceIsReadyAgain() {
        remote.devices.value = listOf(episode())
        show()
        open()
        compose.onNodeWithContentDescription("Stop on Living room TV").performClick()
        compose.waitForIdle()
        // The dialog's Stop, not the one that opened it.
        compose.onNode(hasText("Stop") and !hasContentDescription("Stop on Living room TV")).performClick()
        compose.waitForIdle()
        assertEquals(listOf("s1"), remote.stopped)
        remote.devices.value = listOf(episode().copy(session = null))
        compose.waitForIdle()
        // Listed, ready to play.
        compose.onNodeWithTag(REMOTE_CONTROLS + "app-12").assertDoesNotExist()
        compose.onNodeWithText("Ready to play").assertExists()
        remote.devices.value = emptyList()
        compose.waitForIdle()
        compose.onNodeWithTag(REMOTE_SHEET).assertDoesNotExist()
        compose.onNodeWithTag(REMOTE_BAR).assertDoesNotExist()
    }

    /** The remote as the test drives it: its devices, its clock, and what it was asked. */
    private class FakeRemote : RemoteRepository {
        override val devices = MutableStateFlow<List<OtherDevice>>(emptyList())
        override val sessions = MutableStateFlow<List<RemoteSession>>(emptyList())
        var now = 10_000L
        override val clock: () -> Long = { now }
        val commands = mutableListOf<Pair<String, PlayerCommandRequest>>()
        val stopped = mutableListOf<String>()
        var refreshes = 0
        var failure: Exception? = null

        override suspend fun refresh(): List<RemoteSession> {
            refreshes++
            return devices.value.mapNotNull { it.session }
        }

        override suspend fun command(sessionId: String, command: PlayerCommandRequest) {
            failure?.let { throw it }
            commands += sessionId to command
        }

        override suspend fun stop(sessionId: String) {
            stopped += sessionId
        }

        override suspend fun cast(receiverId: String, request: CastRequest) = Unit

        override fun here(player: Any, sessionId: String?) = Unit
    }

    private companion object {
        const val ORIGIN = "http://127.0.0.1:9"
        const val UPDATED_AT = 1_761_390_000L

        /**
         * An episode on the Living room TV (a receiver unless [receiver] is
         * false), at 19:45 of 53:29, reported 4.2 s before the answer at
         * 10 s, with two audio tracks (the first playing) and two
         * subtitles (none showing).
         */
        fun episode(
            id: String = "s1",
            deviceId: String = "app-12",
            device: String? = "Living room TV",
            state: ProgressState = ProgressState.Playing,
            controls: List<Control> = listOf(
                Control.Play,
                Control.Pause,
                Control.Seek,
                Control.Skip,
                Control.Next,
                Control.Audio,
                Control.Subtitles,
                Control.Quality,
            ),
            audioIndex: Int = 1,
            receiver: Boolean = true,
            quality: String? = "auto",
        ) = OtherDevice(
            deviceId,
            receiver = RemoteDevice("tv-$deviceId", deviceId, device, DeviceKind.Tv, listOf(CastKind.Item, CastKind.Live)).takeIf { receiver },
            session = RemoteSession(
                PlayingSession(
                    sessionId = id,
                    kind = PlayingKind.Item,
                    device = device,
                    deviceId = deviceId,
                    deviceKind = DeviceKind.Tv,
                    item = ItemSummary(
                        204, 2, ItemKind.Episode, "The Second One", index = 2, parentIndex = 1, parentId = 202, grandparentId = 201,
                        grandparentTitle = "A Show",
                    ),
                    state = state,
                    positionMs = 1_185_000,
                    durationMs = 3_209_000,
                    updatedAt = UPDATED_AT,
                    ageMs = 4_200,
                    controls = controls,
                    audioIndex = audioIndex,
                    subtitleIndex = PlayingSession.SUBTITLES_OFF,
                    tracks = SessionTracks(
                        audio = listOf(
                            TrackOption(1, "eng", codec = "eac3", channels = 6, default = true),
                            TrackOption(2, "fre", codec = "aac", channels = 2),
                        ),
                        subtitles = listOf(TrackOption(3, "eng", codec = "subrip"), TrackOption(4, "fre", codec = "subrip", forced = true)),
                    ),
                    quality = quality,
                    qualities = listOf(
                        QualityOption("original", "Original"),
                        QualityOption("auto", "Automatic"),
                        QualityOption("8m", "8 Mbps 1080p"),
                        QualityOption("4m", "4 Mbps 720p"),
                    ),
                ),
                reportedAt = 5_800,
            ),
        )

        /** A movie on the Living room TV, its player obeying [controls]. */
        fun movie(controls: List<Control>): OtherDevice {
            val playing = episode(controls = controls)
            val session = playing.session!!
            val film = ItemSummary(101, 1, ItemKind.Movie, "A Film", year = 2021)
            return playing.copy(session = session.copy(session = session.session.copy(item = film, tracks = SessionTracks())))
        }

        /** A channel on the Kitchen phone, which goes up and down ([controls]), and takes no casts. */
        fun channel(
            now: String? = "The Evening News",
            nowStart: Long? = null,
            nowStop: Long? = null,
            next: String? = null,
            controls: List<Control> = listOf(Control.Next, Control.Previous),
        ) = OtherDevice(
            "app-15",
            session = RemoteSession(
                PlayingSession(
                    sessionId = "s2",
                    kind = PlayingKind.Live,
                    device = "Kitchen phone",
                    deviceId = "app-15",
                    deviceKind = DeviceKind.Phone,
                    channel = PlayingChannel(1, "Pley News HD", number = "101", now = now, nowStart = nowStart, nowStop = nowStop, next = next),
                    state = ProgressState.Playing,
                    controls = controls,
                ),
                reportedAt = 9_100,
            ),
        )

        /** The Bedroom TV, on and playing nothing. */
        fun bedroom() = OtherDevice("app-20", receiver = RemoteDevice("tv-2", "app-20", "Bedroom TV", DeviceKind.Tv, listOf(CastKind.Item)))
    }
}
