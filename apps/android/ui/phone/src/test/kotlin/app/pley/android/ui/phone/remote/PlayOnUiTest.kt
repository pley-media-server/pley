package app.pley.android.ui.phone.remote

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.test.assertHeightIsEqualTo
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertWidthIsEqualTo
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.unit.dp
import app.pley.android.data.OtherDevice
import app.pley.android.data.RemoteSession
import app.pley.android.model.CastKind
import app.pley.android.model.CastProtocol
import app.pley.android.model.CastRequest
import app.pley.android.model.DeviceKind
import app.pley.android.model.DeviceProfileName
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.PlayingChannel
import app.pley.android.model.PlayingKind
import app.pley.android.model.PlayingSession
import app.pley.android.model.RemoteDevice
import app.pley.android.model.UserState
import app.pley.android.network.ApiException
import app.pley.android.screens.item.TrackChoice
import app.pley.android.screens.item.TrackChoices
import app.pley.android.testing.screens.onboarding.FakeOnboardingSource
import app.pley.android.ui.phone.kit.PIN_ERROR_TAG
import app.pley.android.ui.phone.kit.SPINNER_DELAY_MS
import app.pley.android.ui.phone.onboarding.LocalOnboardingSource
import app.pley.android.ui.phone.shell.PhoneNavigation
import app.pley.android.ui.phone.shell.PhoneShell
import app.pley.android.ui.phone.shell.PhoneShellState
import app.pley.android.ui.phone.shell.PhoneStart
import app.pley.android.ui.phone.shell.TestApplication
import app.pley.android.ui.phone.shell.TestGraph
import app.pley.android.ui.phone.theme.PhoneTheme
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Play on…: its button only while another device takes what a page plays,
 * the chooser of those devices, the cast it sends, the remote's bar
 * saying it starts on the device (no sheet coming up over it), and what a
 * refusal says.
 */
@RunWith(RobolectricTestRunner::class)
@Config(application = TestApplication::class)
class PlayOnUiTest {
    @get:Rule
    val compose = createComposeRule()

    private lateinit var state: PhoneShellState
    private lateinit var graph: TestGraph

    /** The shell, signed in, with [devices] on. */
    private fun shell(vararg devices: OtherDevice) {
        graph = TestGraph().also { it.remoteDevices.value = devices.toList() }
        TestApplication.graph = graph
        compose.setContent {
            PhoneTheme {
                CompositionLocalProvider(LocalOnboardingSource provides FakeOnboardingSource()) {
                    PhoneShell(PhoneStart(TestGraph.SERVER, signedIn = true), { graph }) { s, _: PhoneNavigation -> state = s }
                }
            }
        }
        // The remote comes after the first frame, off the main thread.
        compose.waitUntil(5_000) { compose.onAllNodesWithTag(REMOTE_BUTTON).fetchSemanticsNodes().isNotEmpty() }
    }

    /** Play on… asked for [target], as a page's button asks. */
    private fun choose(target: CastTarget) {
        compose.runOnIdle { state.playOn(target) }
        compose.waitForIdle()
    }

    @Test
    fun theButtonShowsOnlyWhileAnotherDeviceTakesWhatThePagePlays() {
        val devices = MutableStateFlow<List<OtherDevice>>(emptyList())
        val asked = mutableListOf<CastTarget>()
        compose.setContent {
            PhoneTheme {
                val on by devices.collectAsState()
                val host = object : PlayOnHost {
                    override val devices: List<OtherDevice> get() = on

                    override fun playOn(target: CastTarget) {
                        asked += target
                    }
                }
                CompositionLocalProvider(LocalPlayOn provides host) {
                    PlayOnButton(CastTarget(itemId = 101))
                }
            }
        }
        compose.onNodeWithTag(PLAY_ON).assertDoesNotExist()
        // A device playing, but taking no casts (a Jellyfin app), or only Live TV.
        devices.value = listOf(kitchen(), bedroom(listOf(CastKind.Live)))
        compose.waitForIdle()
        compose.onNodeWithTag(PLAY_ON).assertDoesNotExist()
        devices.value = listOf(kitchen(), livingRoom())
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Play on…").performClick()
        assertEquals(listOf(CastTarget(itemId = 101)), asked)
    }

    @Test
    fun outsideAShellOrAPlayerItShowsNowhere() {
        compose.setContent { PhoneTheme { PlayOnButton(CastTarget(itemId = 101)) } }
        compose.onNodeWithTag(PLAY_ON).assertDoesNotExist()
    }

    @Test
    fun theChooserListsTheDevicesThatTakeItAndPickingOneSendsItThere() {
        shell(kitchen(), livingRoom(), bedroom(listOf(CastKind.Live)))
        choose(CastTarget(itemId = 101, startMs = 60_000))
        compose.onNodeWithTag(PLAY_ON_SHEET).assertExists()
        compose.onNodeWithText("Play on…").assertExists()
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-12").assertExists()
        // What it plays now, as the remote says it.
        compose.onNodeWithText("Ready to play").assertExists()
        // No receiver, and one of Live TV only: not offered.
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-15").assertDoesNotExist()
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-20").assertDoesNotExist()
        // Nothing here shuffles.
        compose.onNodeWithTag(PLAY_ON_SHUFFLE).assertDoesNotExist()

        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-12").performClick()
        compose.waitForIdle()
        assertEquals(listOf("tv-1" to CastRequest(itemId = 101, startMs = 60_000)), graph.castsSent.toList())
        compose.onNodeWithTag(PLAY_ON_SHEET).assertDoesNotExist()
        // The bar alone follows it: no toast saying the same, and no sheet coming up over them.
        compose.onNodeWithText("Starting on Living room TV…").assertExists()
        compose.onNodeWithTag(REMOTE_SHEET).assertDoesNotExist()
        compose.onNodeWithTag(REMOTE_CONTROLS + "app-12").assertDoesNotExist()
    }

    @Test
    fun whatShufflesIsSentShuffledWhenAsked() {
        shell(livingRoom())
        choose(CastTarget(playlistId = 9, shuffles = true))
        compose.onNodeWithTag(PLAY_ON_SHUFFLE).performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Shuffle on…").assertExists()
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-12").performClick()
        compose.waitForIdle()
        assertEquals(listOf("tv-1" to CastRequest(playlistId = 9, shuffle = true)), graph.castsSent.toList())
    }

    @Test
    fun aChannelIsWatchOnAndGoesToDevicesThatTakeLiveTv() {
        shell(livingRoom(listOf(CastKind.Item)), bedroom(listOf(CastKind.Live)))
        choose(CastTarget.channel(7))
        compose.onNodeWithText("Watch on…").assertExists()
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-12").assertDoesNotExist()
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-20").performClick()
        compose.waitForIdle()
        assertEquals(listOf("tv-2" to CastRequest(channelId = 7)), graph.castsSent.toList())
        compose.onNodeWithText("Starting on Bedroom TV…").assertExists()
    }

    @Test
    fun refusalsAreSaidUnderTheDevicesAndAnotherCanBePicked() {
        shell(livingRoom())
        choose(CastTarget(itemId = 101))
        graph.castFailure = ApiException(404, "not_found", "no such device")
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-12").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Living room TV isn't on any more.").assertExists()
        compose.onNodeWithTag(PLAY_ON_SHEET).assertExists()

        graph.castFailure = ApiException(409, "not_castable", "this device doesn't play titles from a remote")
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-12").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Living room TV can't play that.").assertExists()
        compose.onAllNodesWithText("Living room TV isn't on any more.").fetchSemanticsNodes().let { assertTrue(it.isEmpty()) }

        graph.castFailure = ApiException(429, "too_many_requests", "10 casts a second at most")
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-12").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Too many at once: try again in a moment.").assertExists()

        // Anything else says the server's own words.
        graph.castFailure = ApiException(403, "forbidden", "live TV isn't open to you")
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-12").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Live TV isn't open to you.").assertExists()
        assertTrue(graph.castsSent.isEmpty())
        compose.onNodeWithText("Starting on Living room TV…").assertDoesNotExist()
    }

    @Test
    fun anotherProfilesDeviceShowsWhoseItIsAndACastThereNamesThatProfile() {
        shell(livingRoom(), samsTv())
        choose(CastTarget(itemId = 101))
        // After the profile's own, with the profile it is on, never what it plays.
        compose.onNodeWithText("On Sam's profile").assertExists()
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-31").performClick()
        compose.waitForIdle()
        // This profile has no PIN: no pad, straight there.
        compose.onNodeWithTag(PLAY_ON_PIN).assertDoesNotExist()
        assertEquals(listOf("tv-3" to CastRequest(itemId = 101, profileId = 3, deviceId = "app-31")), graph.castsSent.toList())
        compose.onNodeWithText("Starting on Den TV…").assertExists()
    }

    @Test
    fun withAPinTheCastAsksForItFirstAndSendsIt() {
        shell(samsTv())
        graph.castNeedsPin = true
        choose(CastTarget(itemId = 101))
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-31").performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(PLAY_ON_PIN).assertExists()
        compose.onNodeWithText("Enter your PIN").assertExists()
        compose.onNodeWithText("Den TV switches from Sam's profile to yours.").assertExists()
        assertTrue(graph.castsSent.isEmpty())
        type("1234")
        assertEquals(listOf("tv-3" to CastRequest(itemId = 101, profileId = 3, deviceId = "app-31", pin = "1234")), graph.castsSent.toList())
        compose.onNodeWithTag(PLAY_ON_SHEET).assertDoesNotExist()
        compose.onNodeWithText("Starting on Den TV…").assertExists()
    }

    @Test
    fun aPinTheServerAsksForIsAskedForAndWrongOrLockedOnesAreSaid() {
        shell(samsTv())
        choose(CastTarget(itemId = 101))
        // A PIN set since the profile was read: the server asks, and the pad opens.
        graph.castRefusals += ApiException(403, "pin_needed", "this profile is protected by a PIN")
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-31").performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(PLAY_ON_PIN).assertExists()
        compose.onNodeWithTag(PIN_ERROR_TAG).assertDoesNotExist()

        graph.castRefusals += ApiException(403, "wrong_pin", "wrong PIN")
        type("0000")
        compose.onNodeWithText("Wrong PIN. Try again.").assertExists()
        // Too many: the server says how long to wait.
        graph.castRefusals += ApiException(429, "too_many_requests", "too many attempts, try again in 14 min")
        type("0000")
        compose.onNodeWithText("Too many attempts, try again in 14 min.").assertExists()
        // Locked: the pad stops, and leads back to the devices.
        graph.castRefusals += ApiException(403, "pin_locked", "this profile's PIN is locked")
        type("0000")
        compose.onNodeWithText("Your PIN is locked after too many wrong tries. You or an administrator can reset it.").assertExists()
        compose.onNodeWithTag("pley.pad.1").assertIsNotEnabled()
        assertTrue(graph.castsSent.isEmpty())
        compose.onNodeWithText("Back to the devices").performScrollTo().performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(PLAY_ON_PIN).assertDoesNotExist()
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-31").assertExists()

        // A refusal that isn't the PIN's goes back to the list, and says so there.
        graph.castRefusals += ApiException(403, "pin_needed", "this profile is protected by a PIN")
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-31").performClick()
        compose.waitForIdle()
        graph.castRefusals += ApiException(404, "not_found", "no such device")
        type("1234")
        compose.onNodeWithTag(PLAY_ON_PIN).assertDoesNotExist()
        compose.onNodeWithText("Den TV isn't on any more.").assertExists()
    }

    /** Types [digits] on the PIN pad, then OK. */
    private fun type(digits: String) {
        press(digits)
        compose.onNodeWithTag("pley.pad.ok").performScrollTo().performClick()
        compose.waitForIdle()
    }

    /** Types [digits] on the PIN pad, without OK. */
    private fun press(digits: String) {
        digits.forEach { compose.onNodeWithTag("pley.pad.$it").performScrollTo().performClick() }
        compose.waitForIdle()
    }

    @Test
    fun aGoogleCastOrAirPlayDeviceTakesMoviesAndEpisodesAloneAndSaysWhatItIs() {
        shell(livingRoom(), appleTv(), castTv())
        choose(CastTarget(itemId = 500, shuffles = true))
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-12").assertExists()
        compose.onNodeWithTag(PLAY_ON_DEVICE + APPLE_TV).assertDoesNotExist()
        compose.onNodeWithTag(PLAY_ON_DEVICE + CAST_TV).assertDoesNotExist()
        choose(CastTarget(itemId = 101, video = true))
        compose.onNodeWithText("AirPlay").assertExists()
        compose.onNodeWithText("Google Cast").assertExists()
        // One it can't reach, or that didn't play it, says so.
        graph.castRefusals += ApiException(409, "cast_unreachable", "the device can't be reached")
        compose.onNodeWithTag(PLAY_ON_DEVICE + CAST_TV).performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Bedroom Chromecast can't reach this server.").assertExists()
        graph.castRefusals += ApiException(409, "cast_failed", "the device didn't play it")
        compose.onNodeWithTag(PLAY_ON_DEVICE + CAST_TV).performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Bedroom Chromecast didn't play it.").assertExists()
        assertTrue(graph.castsSent.isEmpty())
    }

    @Test
    fun aGoogleCastOrAirPlayDeviceTakesAChannelWhenItSaysItTakesLiveTv() {
        val castTv = castTv().let { it.copy(receiver = it.receiver?.copy(casts = listOf(CastKind.Item, CastKind.Live))) }
        // The Apple TV comes from a server that casts no channel to it.
        shell(castTv, appleTv())
        choose(CastTarget(channelId = 7))
        compose.onNodeWithTag(PLAY_ON_DEVICE + CAST_TV).assertExists()
        compose.onNodeWithTag(PLAY_ON_DEVICE + APPLE_TV).assertDoesNotExist()
        // With every connection in use it says so: a cast takes none over.
        graph.castRefusals += ApiException(409, "connections_full", "Provider allows one connection at once, and it is in use: watching News.")
        compose.onNodeWithTag(PLAY_ON_DEVICE + CAST_TV).performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Bedroom Chromecast can't play the channel: all Live TV connections are in use.").assertExists()
    }

    @Test
    fun aCastStartingOnADeviceShowsASpinnerOnItsRowUntilTheDeviceTakesIt() {
        shell(castTv(), livingRoom())
        choose(CastTarget(itemId = 101, video = true))
        // A Google Cast device answers once it plays: seconds later.
        val gate = kotlinx.coroutines.CompletableDeferred<Unit>()
        graph.castGate = gate
        compose.onNodeWithTag(PLAY_ON_DEVICE + CAST_TV).performClick()
        compose.mainClock.advanceTimeBy(SPINNER_DELAY_MS + 100)
        compose.waitForIdle()
        // Inside the row, which reads as one.
        compose.onNodeWithTag(PLAY_ON_STARTING, useUnmergedTree = true).assertExists()
        compose.onNodeWithContentDescription("Starting on Bedroom Chromecast…", useUnmergedTree = true).assertExists()
        // One cast at a time: the other devices wait.
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-12").assertIsNotEnabled()
        gate.complete(Unit)
        compose.waitForIdle()
        assertEquals(listOf(CAST_TV to CastRequest(itemId = 101)), graph.castsSent.toList())
        compose.onNodeWithTag(PLAY_ON_SHEET).assertDoesNotExist()
    }

    @Test
    fun aCastOnItsWayGoesOnWhenTheChooserIsClosedButAsksForNoCodeThen() {
        shell(castTv(), appleTv())
        choose(CastTarget(itemId = 101, video = true))
        val gate = kotlinx.coroutines.CompletableDeferred<Unit>()
        graph.castGate = gate
        compose.onNodeWithTag(PLAY_ON_DEVICE + CAST_TV).performClick()
        compose.waitForIdle()
        // Closed, as a swipe or Back closes it, before the device took it.
        compose.runOnIdle { state.playingOn = null }
        compose.waitForIdle()
        compose.onNodeWithTag(PLAY_ON_SHEET).assertDoesNotExist()
        gate.complete(Unit)
        compose.waitForIdle()
        assertEquals(listOf(CAST_TV to CastRequest(itemId = 101)), graph.castsSent.toList())
        compose.onNodeWithText("Starting on Bedroom Chromecast…").assertExists()

        // An AirPlay device asking for its code once the chooser is gone isn't asked to show one: no pad to type it
        // on. A toast says to pick it again.
        choose(CastTarget(itemId = 102, video = true))
        val second = kotlinx.coroutines.CompletableDeferred<Unit>()
        graph.castGate = second
        graph.castRefusals += ApiException(409, "pairing_needed", "the device wants a PIN")
        compose.onNodeWithTag(PLAY_ON_DEVICE + APPLE_TV).performClick()
        compose.waitForIdle()
        compose.runOnIdle { state.playingOn = null }
        compose.waitForIdle()
        second.complete(Unit)
        compose.waitForIdle()
        assertTrue(graph.codesShown.isEmpty())
        compose.onNodeWithTag(PLAY_ON_PIN).assertDoesNotExist()
        compose.onNodeWithText("Den Apple TV wants pairing first: choose it in Play on… again.").assertExists()
    }

    @Test
    fun aChooserOpenedAgainWhileACastStartsShowsItAndWaits() {
        shell(castTv(), livingRoom())
        choose(CastTarget(itemId = 101, video = true))
        val gate = kotlinx.coroutines.CompletableDeferred<Unit>()
        graph.castGate = gate
        compose.onNodeWithTag(PLAY_ON_DEVICE + CAST_TV).performClick()
        compose.waitForIdle()
        compose.runOnIdle { state.playingOn = null }
        compose.waitForIdle()

        // Opened again, for the same title or another: the cast still starting shows, and nothing else can go.
        choose(CastTarget(itemId = 102, video = true))
        compose.mainClock.advanceTimeBy(SPINNER_DELAY_MS + 100)
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Starting on Bedroom Chromecast…", useUnmergedTree = true).assertExists()
        compose.onNodeWithTag(PLAY_ON_DEVICE + CAST_TV).assertIsNotEnabled()
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-12").assertIsNotEnabled()
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-12").performClick()
        compose.waitForIdle()

        // The first went, and closes no chooser opened since: it was sent from the one that closed.
        gate.complete(Unit)
        compose.waitForIdle()
        assertEquals(listOf(CAST_TV to CastRequest(itemId = 101)), graph.castsSent.toList())
        compose.onNodeWithTag(PLAY_ON_SHEET).assertExists()
        compose.onNodeWithTag(PLAY_ON_DEVICE + "app-12").assertIsEnabled()
    }

    @Test
    fun aCastLandingLateClosesNoChooserOpenedSinceForTheSameTitle() {
        shell(castTv(), livingRoom())
        val target = CastTarget(itemId = 101, video = true)
        choose(target)
        val gate = kotlinx.coroutines.CompletableDeferred<Unit>()
        graph.castGate = gate
        compose.onNodeWithTag(PLAY_ON_DEVICE + CAST_TV).performClick()
        compose.waitForIdle()
        compose.runOnIdle { state.playingOn = null }
        compose.waitForIdle()
        choose(target)
        gate.complete(Unit)
        compose.waitForIdle()
        compose.onNodeWithTag(PLAY_ON_SHEET).assertExists()
    }

    @Test
    fun aRefusalComingWhileTheChooserIsOpenAgainIsSaidInIt() {
        shell(castTv(), livingRoom())
        choose(CastTarget(itemId = 101, video = true))
        val gate = kotlinx.coroutines.CompletableDeferred<Unit>()
        graph.castGate = gate
        graph.castRefusals += ApiException(409, "cast_failed", "the device didn't play it")
        compose.onNodeWithTag(PLAY_ON_DEVICE + CAST_TV).performClick()
        compose.waitForIdle()
        compose.runOnIdle { state.playingOn = null }
        compose.waitForIdle()
        choose(CastTarget(itemId = 101, video = true))
        gate.complete(Unit)
        compose.waitForIdle()
        // Under the devices of the chooser open now, as the one that sent it would say it, and no toast under the sheet.
        compose.onNodeWithTag(PLAY_ON_SHEET).assertExists()
        compose.onAllNodesWithText("Bedroom Chromecast didn't play it.").assertCountEquals(1)
        assertTrue(state.toasts.current.isEmpty())
        compose.onNodeWithTag(PLAY_ON_DEVICE + CAST_TV).assertIsEnabled()
    }

    @Test
    fun aRefusalAfterTheChooserClosedIsAToast() {
        shell(castTv())
        choose(CastTarget(itemId = 101, video = true))
        val gate = kotlinx.coroutines.CompletableDeferred<Unit>()
        graph.castGate = gate
        graph.castRefusals += ApiException(409, "cast_unreachable", "the device can't be reached")
        compose.onNodeWithTag(PLAY_ON_DEVICE + CAST_TV).performClick()
        compose.waitForIdle()
        compose.runOnIdle { state.playingOn = null }
        compose.waitForIdle()
        gate.complete(Unit)
        compose.waitForIdle()
        // What the chooser would have said under its devices.
        compose.onNodeWithText("Bedroom Chromecast can't reach this server.").assertExists()
        compose.onNodeWithTag(PLAY_ON_SHEET).assertDoesNotExist()
    }

    @Test
    fun musicPlayingHerePausesOnceTheDeviceTookTheCastNotWhileItStartsNorOnARefusal() {
        shell(castTv())
        choose(CastTarget(itemId = 101, video = true))
        val refused = kotlinx.coroutines.CompletableDeferred<Unit>()
        graph.castGate = refused
        graph.castRefusals += ApiException(409, "cast_unreachable", "the device can't be reached")
        compose.onNodeWithTag(PLAY_ON_DEVICE + CAST_TV).performClick()
        compose.waitForIdle()
        assertEquals(0, graph.musicPauses)
        refused.complete(Unit)
        compose.waitForIdle()
        compose.onNodeWithText("Bedroom Chromecast can't reach this server.").assertExists()
        assertEquals(0, graph.musicPauses)

        val took = kotlinx.coroutines.CompletableDeferred<Unit>()
        graph.castGate = took
        compose.onNodeWithTag(PLAY_ON_DEVICE + CAST_TV).performClick()
        compose.waitForIdle()
        assertEquals(0, graph.musicPauses)
        took.complete(Unit)
        compose.waitForIdle()
        compose.onNodeWithText("Starting on Bedroom Chromecast…").assertExists()
        assertEquals(1, graph.musicPauses)
    }

    @Test
    fun aCastThatWentBeforeTheMusicWasReachedPausesItOnceItIs() {
        shell(livingRoom())
        // As in a shell made again with the activity: the cast's news comes before the music is reached.
        val music = graph.player.music
        compose.runOnIdle {
            state.music = null
            state.castSent("app-12")
        }
        assertEquals(0, graph.musicPauses)
        compose.runOnIdle { state.music = music }
        assertEquals(1, graph.musicPauses)
        // Once.
        compose.runOnIdle { state.music = music }
        assertEquals(1, graph.musicPauses)
    }

    @Test
    fun aCastFromTheLivePlayerPausesMusicHereToo() {
        shell(livingRoom())
        CastsSent.tell("app-12", "Living room TV")
        compose.waitForIdle()
        compose.onNodeWithText("Starting on Living room TV…").assertExists()
        assertEquals(1, graph.musicPauses)
    }

    @Test
    fun aChooserClosedWhileTheDeviceIsAskedForItsCodeSaysToPickItAgain() {
        shell(appleTv())
        choose(CastTarget(itemId = 101, video = true))
        graph.castRefusals += ApiException(409, "pairing_needed", "the device wants a PIN")
        val gate = kotlinx.coroutines.CompletableDeferred<Unit>()
        graph.codeGate = gate
        compose.onNodeWithTag(PLAY_ON_DEVICE + APPLE_TV).performClick()
        compose.waitForIdle()
        compose.runOnIdle { state.playingOn = null }
        compose.waitForIdle()
        gate.complete(Unit)
        compose.waitForIdle()
        assertEquals(listOf(APPLE_TV), graph.codesShown)
        compose.onNodeWithTag(PLAY_ON_PIN).assertDoesNotExist()
        compose.onNodeWithText("Den Apple TV wants pairing first: choose it in Play on… again.").assertExists()
    }

    @Test
    fun anAppleTvAskingForPairingShowsItsCodeWhichGoesAtItsFourthDigitThenTheCastGoesAgain() {
        shell(appleTv())
        choose(CastTarget(itemId = 101, video = true))
        graph.castRefusals += ApiException(409, "pairing_needed", "the device wants a PIN")
        compose.onNodeWithTag(PLAY_ON_DEVICE + APPLE_TV).performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(PLAY_ON_PIN).assertExists()
        compose.onNodeWithText("Pair with Den Apple TV").assertExists()
        // By its name: a Mac takes AirPlay too.
        compose.onNodeWithText("Type the code Den Apple TV shows").assertExists()
        compose.onNodeWithText("No code showing? Show it again").assertExists()
        assertEquals(listOf(APPLE_TV), graph.codesShown)

        // A wrong code, sent at its fourth digit with no OK: the pad says so.
        graph.pairingRefusals += ApiException(403, "wrong_pin", "that isn't the PIN the device shows")
        press("1111")
        compose.onNodeWithText("That isn't the code Den Apple TV shows.").assertExists()
        // No code showing: it shows one again, and what was typed goes.
        press("48")
        compose.onNodeWithTag(PLAY_ON_SHOW_CODE).performScrollTo().performClick()
        compose.waitForIdle()
        assertEquals(listOf(APPLE_TV, APPLE_TV), graph.codesShown)
        compose.onNodeWithText("That isn't the code Den Apple TV shows.").assertDoesNotExist()

        press("4821")
        assertEquals(listOf(APPLE_TV to "1111", APPLE_TV to "4821"), graph.codesTyped)
        assertEquals(listOf(APPLE_TV to CastRequest(itemId = 101)), graph.castsSent.toList())
        compose.onNodeWithTag(PLAY_ON_PIN).assertDoesNotExist()
    }

    @Test
    fun anAnswerAfterBackClosedThePadDoesntOpenItAgain() {
        shell(appleTv())
        choose(CastTarget(itemId = 101, video = true))
        graph.castRefusals += ApiException(409, "pairing_needed", "the device wants a PIN")
        compose.onNodeWithTag(PLAY_ON_DEVICE + APPLE_TV).performClick()
        compose.waitForIdle()
        val gate = kotlinx.coroutines.CompletableDeferred<Unit>()
        graph.pairingGate = gate
        graph.pairingRefusals += ApiException(403, "wrong_pin", "that isn't the PIN the device shows")
        press("1111")
        compose.onNodeWithContentDescription("Back to the devices").performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(PLAY_ON_PIN).assertDoesNotExist()
        gate.complete(Unit)
        compose.waitForIdle()
        compose.onNodeWithTag(PLAY_ON_PIN).assertDoesNotExist()
        compose.onNodeWithTag(PLAY_ON_DEVICE + APPLE_TV).assertExists()
    }

    @Test
    fun aCodeOfALengthNotKnownGoesWithOkAt4DigitsAtMostAndARefusalOtherThanAWrongCodeGoesBackToTheList() {
        shell(appleTv())
        graph.codeLength = null
        choose(CastTarget(itemId = 101, video = true))
        graph.castRefusals += ApiException(409, "pairing_needed", "the device wants a PIN")
        compose.onNodeWithTag(PLAY_ON_DEVICE + APPLE_TV).performClick()
        compose.waitForIdle()
        // The server takes 4 digits: no more are typed, and nothing goes without OK.
        press("482193")
        assertTrue(graph.codesTyped.isEmpty())
        graph.pairingRefusals += ApiException(409, "pairing_failed", "the device didn't pair")
        compose.onNodeWithTag("pley.pad.ok").performScrollTo().performClick()
        compose.waitForIdle()
        assertEquals(listOf(APPLE_TV to "4821"), graph.codesTyped)
        compose.onNodeWithTag(PLAY_ON_PIN).assertDoesNotExist()
        compose.onNodeWithText("Den Apple TV didn't pair.").assertExists()
    }

    @Test
    fun aCastFromTheLivePlayerIsFollowedByTheRemotesBar() {
        shell(livingRoom())
        CastsSent.tell("app-12", "Living room TV")
        compose.waitForIdle()
        compose.onNodeWithText("Starting on Living room TV…").assertExists()
        compose.onNodeWithTag(REMOTE_SHEET).assertDoesNotExist()
        assertNull(CastsSent.pending.value)
    }

    @Test
    fun whatATitleSendsIsWhatItsPlayPlays() {
        val movie = ItemSummary(101, 1, ItemKind.Movie, "A Film", durationMs = 6_000_000)
        assertEquals(CastTarget(itemId = 101, video = true), CastTarget.of(movie))
        // Being watched: where Play resumes it.
        assertEquals(CastTarget(itemId = 101, startMs = 600_000, video = true), CastTarget.of(movie.copy(userState = UserState(positionMs = 600_000))))
        // Watched: from the start, as Play.
        assertEquals(CastTarget(itemId = 101, video = true), CastTarget.of(movie.copy(userState = UserState(positionMs = 600_000, watched = true))))
        // A show plays its next episode there, where that one was left.
        assertEquals(CastTarget(itemId = 201), CastTarget.of(ItemSummary(201, 2, ItemKind.Show, "A Show", userState = UserState(positionMs = 5_000))))
        assertEquals(CastTarget(itemId = 500, shuffles = true), CastTarget.of(ItemSummary(500, 3, ItemKind.Album, "An Album")))
        assertEquals(CastTarget(itemId = 303), CastTarget.of(ItemSummary(303, 3, ItemKind.Track, "A Track")))
        assertNull(CastTarget.of(ItemSummary(9, 1, ItemKind.Extra, "A Trailer")))
        assertEquals(CastRequest(itemId = 500), CastTarget(itemId = 500, shuffles = true).request(shuffle = false))
        assertEquals(CastRequest(itemId = 500, shuffle = true), CastTarget(itemId = 500, shuffles = true).request(shuffle = true))
    }

    @Test
    fun aTitlesPageSendsTheTracksItsPickersShow() {
        val movie = ItemSummary(101, 1, ItemKind.Movie, "A Film", durationMs = 6_000_000)
        fun tracks(audio: Int?, subtitle: Int?, subtitles: Boolean = true) = TrackChoices(
            versions = emptyList(),
            audio = listOf(TrackChoice(1, "English", audio == 1), TrackChoice(2, "French", audio == 2)),
            subtitles = if (subtitles) listOf(TrackChoice(3, "English", subtitle == 3)) else emptyList(),
            versionId = 11,
            automaticAudio = audio == null,
            automaticLanguage = "English",
            audioIndex = audio,
            subtitleIndex = subtitle,
        )
        assertEquals(CastTarget(itemId = 101, audioIndex = 2, subtitleIndex = 3, video = true), CastTarget.page(movie, tracks(audio = 2, subtitle = 3)))
        // Automatic audio names no track, as Play here; subtitles off are -1.
        assertEquals(CastTarget(itemId = 101, subtitleIndex = PlayingSession.SUBTITLES_OFF, video = true), CastTarget.page(movie, tracks(audio = null, subtitle = null)))
        // A file without subtitles names none; a page with nothing to pick, nothing.
        assertEquals(CastTarget(itemId = 101, audioIndex = 1, video = true), CastTarget.page(movie, tracks(audio = 1, subtitle = null, subtitles = false)))
        assertEquals(CastTarget(itemId = 101, video = true), CastTarget.page(movie, null))
        // A show's page plays its next episode, with that one's own tracks.
        assertEquals(CastTarget(itemId = 201), CastTarget.page(ItemSummary(201, 2, ItemKind.Show, "A Show"), tracks(audio = 2, subtitle = 3)))
        assertEquals(
            CastRequest(itemId = 101, audioIndex = 2, subtitleIndex = -1),
            CastTarget(itemId = 101, audioIndex = 2, subtitleIndex = -1).request(shuffle = false),
        )
    }

    @Test
    fun theButtonBesidePlayIsASquareAsTallAsIt() {
        compose.setContent {
            PhoneTheme {
                val host = object : PlayOnHost {
                    override val devices: List<OtherDevice> = listOf(livingRoom())

                    override fun playOn(target: CastTarget) = Unit
                }
                CompositionLocalProvider(LocalPlayOn provides host) {
                    PlayOnButton(CastTarget(itemId = 101))
                }
            }
        }
        compose.onNodeWithTag(PLAY_ON).assertWidthIsEqualTo(48.dp).assertHeightIsEqualTo(48.dp)
    }

    private companion object {
        /** The Living room TV, on and playing nothing, taking [casts]. */
        fun livingRoom(casts: List<CastKind> = listOf(CastKind.Item, CastKind.Live)) =
            OtherDevice("app-12", receiver = RemoteDevice("tv-1", "app-12", "Living room TV", DeviceKind.Tv, casts))

        /** The Den TV, on Sam's profile, taking titles and channels. */
        fun samsTv() = OtherDevice(
            "app-31",
            receiver = RemoteDevice("tv-3", "app-31", "Den TV", DeviceKind.Tv, listOf(CastKind.Item, CastKind.Live), profile = DeviceProfileName(3, "Sam")),
        )

        const val APPLE_TV = "airplay:a1b2c3d4e5f6"
        const val CAST_TV = "cast:5f1a8e0c"

        /** An Apple TV the server plays on itself. */
        fun appleTv() = OtherDevice(APPLE_TV, receiver = RemoteDevice(APPLE_TV, APPLE_TV, "Den Apple TV", DeviceKind.Tv, listOf(CastKind.Item), protocol = CastProtocol.AirPlay))

        /** A Google Cast device the server plays on itself. */
        fun castTv() = OtherDevice(CAST_TV, receiver = RemoteDevice(CAST_TV, CAST_TV, "Bedroom Chromecast", DeviceKind.Tv, listOf(CastKind.Item), protocol = CastProtocol.Cast))

        /** The Bedroom TV, on and playing nothing, taking [casts]. */
        fun bedroom(casts: List<CastKind>) = OtherDevice("app-20", receiver = RemoteDevice("tv-2", "app-20", "Bedroom TV", DeviceKind.Tv, casts))

        /** A channel on the Kitchen phone, an app that takes no casts. */
        fun kitchen() = OtherDevice(
            "app-15",
            session = RemoteSession(
                PlayingSession("s2", PlayingKind.Live, "Kitchen phone", "app-15", DeviceKind.Phone, channel = PlayingChannel(1, "Pley News HD", number = "101")),
                reportedAt = 0,
            ),
        )
    }
}
