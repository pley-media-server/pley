package app.pley.android.player.engine

import app.pley.android.caps.CapsDetails
import app.pley.android.caps.DisplayModeInfo
import app.pley.android.caps.DisplayState
import app.pley.android.caps.MatchContent
import app.pley.android.model.DynamicRange
import app.pley.android.model.MediaStream
import app.pley.android.model.StreamKind
import app.pley.android.model.caps.DolbyVisionCaps
import app.pley.android.testing.caps.TestDevices
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ModeChoiceTest {
    private val film = 2 * 60 * 60 * 1000L

    private fun request(
        fps: Double?,
        setting: RefreshRateMatching = RefreshRateMatching.Auto,
        hdr: Set<Int> = emptySet(),
        dolbyVision: Boolean = false,
        durationMs: Long = film,
        dvSwitchAllowed: Boolean = false,
        gaveUp: Boolean = false,
    ) = MatchRequest(fps, durationMs, hdr, dolbyVision, setting, dvSwitchAllowed, gaveUp)

    private fun decide(display: DisplayState, request: MatchRequest, sdk: Int = 34) = ModeChoice.decide(display, request, sdk)

    private fun switchTo(id: Int) = ModeDecision.Switch(id, STREAMER.modes.first { it.id == id }.refreshRate)

    /**
     * A TV that shows Dolby Vision at 23.976 Hz, not at its home screen's
     * 60 Hz: Dolby Vision is shown where the video plays.
     */
    @Test
    fun dolbyVisionIsShownInTheModeTheVideoPlaysIn() {
        val modes = listOf(
            DisplayModeInfo(1, 3840, 2160, 60f, hdrTypes = listOf(2)),
            DisplayModeInfo(2, 3840, 2160, 23.976f, hdrTypes = listOf(1, 2)),
        )
        val display = DisplayState(modes, activeModeId = 1, hdrCapabilities = listOf(2), matchContent = MatchContent.Always)
        val withDv = listOf(DynamicRange.Hdr10, DynamicRange.DolbyVision)
        fun caps(state: DisplayState, screen: List<DynamicRange> = withDv) =
            CapsDetails(input = TestDevices.googleTvStreamer.copy(display = state), screenHdrTypes = screen)
        val film = request(23.976, hdr = setOf(ModeChoice.HDR_DOLBY_VISION), dolbyVision = true)
        assertEquals("switched to 23.976 Hz", true, ModeChoice.dolbyVisionShown(caps(display), film, sdk = 34))
        assertEquals("no switch: the home screen's mode", false, ModeChoice.dolbyVisionShown(caps(display), null, sdk = 34))
        val never = display.copy(matchContent = MatchContent.Never)
        assertEquals("the device's setting is Never", false, ModeChoice.dolbyVisionShown(caps(never), film, sdk = 34))
        assertEquals("not on the screen at all", false, ModeChoice.dolbyVisionShown(caps(display, listOf(DynamicRange.Hdr10)), film, sdk = 34))
        // A quirk adds Dolby Vision, which no mode lists: shown in every one.
        val unlisted = display.copy(modes = modes.map { it.copy(hdrTypes = listOf(2)) })
        assertEquals(true, ModeChoice.dolbyVisionShown(caps(unlisted), null, sdk = 34))
        assertNull("no probe", ModeChoice.dolbyVisionShown(CapsDetails(), film, sdk = 34))
        // Turned off in the system's settings, as seen in a mode that lists it: in no mode.
        val off = setOf(ModeChoice.HDR_DOLBY_VISION)
        assertEquals(false, ModeChoice.dolbyVisionShown(caps(display), film, sdk = 34, turnedOff = off))
        assertEquals(false, ModeChoice.dolbyVisionShown(caps(unlisted), null, sdk = 34, turnedOff = off))
        // The display as it is now, rather than the probe's.
        val at24 = display.copy(activeModeId = 2, hdrCapabilities = listOf(1, 2))
        assertEquals(true, ModeChoice.dolbyVisionShown(caps(display), null, sdk = 34, display = at24))
    }

    @Test
    fun aDisplayMatchIsAskedForFilmsOnTvsOnly() {
        val film = MediaStream(index = 0, kind = StreamKind.Video, codec = "hevc", frameRate = 23.976)
        val hours = 2 * 60 * 60 * 1000L
        assertEquals(23.976, ModeChoice.matchFor(true, film, hours, null, RefreshRateMatching.Auto)?.fps)
        assertEquals(true, ModeChoice.matchFor(true, film, hours, null, RefreshRateMatching.Auto, gaveUp = true)?.gaveUp)
        assertNull("a phone", ModeChoice.matchFor(false, film, hours, null, RefreshRateMatching.Auto))
        assertNull("turned off", ModeChoice.matchFor(true, film, hours, null, RefreshRateMatching.Off))
        assertNull("under 10 minutes", ModeChoice.matchFor(true, film, ModeChoice.MIN_DURATION_MS - 1, null, RefreshRateMatching.Auto))
        assertNull("no frame rate", ModeChoice.matchFor(true, film.copy(frameRate = null), hours, null, RefreshRateMatching.Auto))
        assertNull("no video", ModeChoice.matchFor(true, null, hours, null, RefreshRateMatching.Auto))
    }

    @Test
    fun theStreamerSwitchesToEachFilmAndTvRate() {
        val always = STREAMER.copy(matchContent = MatchContent.Always)
        val table = mapOf(
            23.976 to 46,
            24.0 to 47,
            25.0 to 48,
            29.97 to 49,
            50.0 to 52,
            59.94 to 55,
        )
        table.forEach { (fps, id) ->
            assertEquals("$fps fps", switchTo(id), decide(always, request(fps)))
        }
    }

    @Test
    fun sixtyAndThirtyAlreadyMatchSixtyHertz() {
        val always = STREAMER.copy(matchContent = MatchContent.Always)
        assertEquals(ModeDecision.Keep(KeepReason.AlreadyMatching), decide(always, request(60.0)))
        assertEquals(ModeDecision.Keep(KeepReason.AlreadyMatching), decide(always, request(30.0)))
    }

    @Test
    fun filmRatesAsTheServerRoundsThemStillMatch() {
        val always = STREAMER.copy(matchContent = MatchContent.Always)
        assertEquals(switchTo(46), decide(always, request(24000.0 / 1001)))
        assertEquals(switchTo(46), decide(always, request(23.98)))
        assertEquals(switchTo(55), decide(always, request(60000.0 / 1001)))
    }

    @Test
    fun twentyThreeNineSevenSixNeverMatchesTwentyFour() {
        val tv = display(active = 2, mode(1, 24.000002f), mode(2, 60.000004f))
        assertEquals(ModeDecision.Keep(KeepReason.NoMatch), decide(tv, request(23.976)))
        assertNull(ModeChoice.multipleOf(24.000002f, 23.976))
        assertNull(ModeChoice.multipleOf(120f, 23.976))
    }

    @Test
    fun multiplesUpTo120HzMatchWhenTheExactRateIsMissing() {
        val tv = display(active = 1, mode(1, 60f), mode(2, 119.88012f), mode(3, 120.00001f))
        assertEquals(ModeDecision.Switch(2, 119.88012f), decide(tv, request(23.976)))
        assertEquals(ModeDecision.Switch(3, 120.00001f), decide(tv, request(24.0)))
        assertEquals(5, ModeChoice.multipleOf(119.88012f, 23.976))
        assertEquals(2, ModeChoice.multipleOf(59.94006f, 29.97))
        assertEquals(2, ModeChoice.multipleOf(50f, 25.0))
        assertNull(ModeChoice.multipleOf(60f, 29.97))
    }

    @Test
    fun theExactRateWinsOverItsMultiples() {
        val tv = display(active = 1, mode(1, 60f), mode(2, 50f), mode(3, 25f), mode(4, 100f))
        assertEquals(ModeDecision.Switch(3, 25f), decide(tv, request(25.0)))
        val noExact = display(active = 1, mode(1, 60f), mode(4, 100f), mode(2, 50f))
        assertEquals(ModeDecision.Switch(2, 50f), decide(noExact, request(25.0)))
    }

    @Test
    fun onlyModesOfTheSameSizeAreCandidates() {
        val tv = display(active = 1, mode(1, 60f, 3840, 2160), mode(2, 24f, 1920, 1080))
        assertEquals(ModeDecision.Keep(KeepReason.NoMatch), decide(tv, request(24.0)))
    }

    @Test
    fun onAndroid14ModesMustListTheVideosHdrType() {
        val tv = display(
            active = 1,
            mode(1, 60f, hdr = listOf(2, 3)),
            mode(2, 23.976f, hdr = listOf(2)),
            mode(3, 24f, hdr = listOf(1, 2)),
        )
        // Dolby Vision output: the 23.976 Hz mode doesn't list it.
        assertEquals(ModeDecision.Keep(KeepReason.NoMatch), decide(tv, request(23.976, hdr = setOf(1), dolbyVision = true)))
        assertEquals(ModeDecision.Switch(3, 24f), decide(tv, request(24.0, hdr = setOf(1), dolbyVision = true)))
        // HDR10+ is happy with an HDR10 mode.
        assertEquals(ModeDecision.Switch(2, 23.976f), decide(tv, request(23.976, hdr = setOf(4, 2))))
        // HLG at 23.976: no mode lists it.
        assertEquals(ModeDecision.Keep(KeepReason.NoMatch), decide(tv, request(23.976, hdr = setOf(3))))
        // Before Android 14 modes don't list types, so any mode goes.
        assertEquals(ModeDecision.Switch(2, 23.976f), decide(tv.copy(modes = tv.modes.map { it.copy(hdrTypes = null) }), request(23.976, hdr = setOf(3)), sdk = 33))
    }

    @Test
    fun aMatchingModeWithoutTheHdrTypeIsLeftForOneThatHasIt() {
        val tv = display(active = 1, mode(1, 24f, hdr = emptyList()), mode(2, 24f, hdr = listOf(2)))
        assertEquals(ModeDecision.Switch(2, 24f), decide(tv, request(24.0, hdr = setOf(2))))
    }

    @Test
    fun autoFollowsTheDevicesSettingFromAndroid12() {
        assertEquals(ModeDecision.Keep(KeepReason.SystemNever), decide(STREAMER, request(23.976)))
        assertEquals(switchTo(46), decide(STREAMER.copy(matchContent = MatchContent.Always), request(23.976)))
        assertEquals(switchTo(46), decide(STREAMER.copy(matchContent = MatchContent.Unknown), request(23.976)))
        // Seamless only: the Streamer's 4K60 mode lists no seamless rates.
        assertEquals(ModeDecision.Keep(KeepReason.NoMatch), decide(STREAMER.copy(matchContent = MatchContent.SeamlessOnly), request(23.976)))
    }

    @Test
    fun seamlessOnlyPicksAmongTheAlternativeRates() {
        // The Note20 Ultra at 60 Hz switches seamlessly to 48, 96 and 120 Hz.
        assertEquals(ModeDecision.Switch(6, 48.000004f), decide(NOTE20, request(24.0), sdk = 33))
        val noAlternatives = NOTE20.copy(modes = NOTE20.modes.map { it.copy(alternativeRefreshRates = emptyList()) })
        assertEquals(ModeDecision.Keep(KeepReason.NoMatch), decide(noAlternatives, request(24.0), sdk = 33))
    }

    @Test
    fun belowAndroid12AutoAlwaysSwitches() {
        assertEquals(switchTo(46), decide(STREAMER, request(23.976), sdk = 30))
    }

    @Test
    fun alwaysIgnoresTheDevicesSettingAndPastTimeouts() {
        assertEquals(switchTo(46), decide(STREAMER, request(23.976, setting = RefreshRateMatching.Always, gaveUp = true)))
        assertEquals(ModeDecision.Keep(KeepReason.GaveUp), decide(STREAMER.copy(matchContent = MatchContent.Always), request(23.976, gaveUp = true)))
    }

    @Test
    fun offShortAndUnknownRatesKeepTheMode() {
        val always = STREAMER.copy(matchContent = MatchContent.Always)
        assertEquals(ModeDecision.Keep(KeepReason.Off), decide(always, request(23.976, setting = RefreshRateMatching.Off)))
        assertEquals(ModeDecision.Keep(KeepReason.Short), decide(always, request(23.976, durationMs = 9 * 60 * 1000L)))
        assertEquals(switchTo(46), decide(always, request(23.976, durationMs = ModeChoice.MIN_DURATION_MS)))
        assertEquals(ModeDecision.Keep(KeepReason.UnknownRate), decide(always, request(null)))
        assertEquals(ModeDecision.Keep(KeepReason.NoDisplay), ModeChoice.decide(null, request(23.976), 34))
    }

    @Test
    fun dolbyVisionBelowAndroid14SwitchesOnlyOnAllowedModels() {
        val old = STREAMER.copy(matchContent = MatchContent.Always, modes = STREAMER.modes.map { it.copy(hdrTypes = null) })
        assertEquals(ModeDecision.Keep(KeepReason.DolbyVisionOldAndroid), decide(old, request(23.976, hdr = setOf(1), dolbyVision = true), sdk = 33))
        assertEquals(switchTo(46), decide(old, request(23.976, hdr = setOf(1), dolbyVision = true, dvSwitchAllowed = true), sdk = 33))
        // The HDR10 base layer of a Dolby Vision file isn't Dolby Vision output.
        assertEquals(switchTo(46), decide(old, request(23.976, hdr = setOf(2)), sdk = 33))
    }

    @Test
    fun dolbyVisionComesOutAsDolbyVisionOnlyOnDolbyVisionScreensWithTheProfile() {
        val p8 = video(DynamicRange.DolbyVision, dvProfile = 8, blCompat = 1)
        val hdr10Screen = CapsDetails(screenHdrTypes = listOf(DynamicRange.Hdr10), dolbyVisionProfiles = listOf(DolbyVisionCaps(5), DolbyVisionCaps(8)))
        val onHdr10 = ModeChoice.requestFor(p8, film, hdr10Screen, RefreshRateMatching.Auto)
        assertEquals(setOf(ModeChoice.HDR_HDR10), onHdr10.hdrTypes)
        assertFalse(onHdr10.dolbyVision)

        val dvScreen = hdr10Screen.copy(screenHdrTypes = listOf(DynamicRange.DolbyVision, DynamicRange.Hdr10))
        val onDv = ModeChoice.requestFor(p8, film, dvScreen, RefreshRateMatching.Auto)
        assertEquals(setOf(ModeChoice.HDR_DOLBY_VISION), onDv.hdrTypes)
        assertTrue(onDv.dolbyVision)

        val p7 = ModeChoice.requestFor(video(DynamicRange.DolbyVision, dvProfile = 7, blCompat = 6), film, dvScreen, RefreshRateMatching.Auto)
        assertEquals(setOf(ModeChoice.HDR_HDR10), p7.hdrTypes)
        val hlgBase = ModeChoice.requestFor(video(DynamicRange.DolbyVision, dvProfile = 8, blCompat = 4), film, hdr10Screen, RefreshRateMatching.Auto)
        assertEquals(setOf(ModeChoice.HDR_HLG), hlgBase.hdrTypes)
        val hdr10Plus = ModeChoice.requestFor(video(DynamicRange.Hdr10Plus), film, hdr10Screen, RefreshRateMatching.Auto)
        assertEquals(setOf(ModeChoice.HDR_HDR10_PLUS, ModeChoice.HDR_HDR10), hdr10Plus.hdrTypes)
        assertEquals(emptySet<Int>(), ModeChoice.requestFor(video(DynamicRange.Sdr), film, hdr10Screen, RefreshRateMatching.Auto).hdrTypes)
    }

    private fun video(range: DynamicRange, dvProfile: Int? = null, blCompat: Int? = null) = MediaStream(
        index = 0,
        kind = StreamKind.Video,
        codec = "hevc",
        frameRate = 23.976,
        dynamicRange = range,
        dvProfile = dvProfile,
        dvBlCompat = blCompat,
    )

    private fun mode(id: Int, rate: Float, width: Int = 3840, height: Int = 2160, hdr: List<Int>? = listOf(2)) =
        DisplayModeInfo(id = id, width = width, height = height, refreshRate = rate, hdrTypes = hdr)

    private fun display(active: Int, vararg modes: DisplayModeInfo) =
        DisplayState(modes = modes.toList(), activeModeId = active, matchContent = MatchContent.Always)

    companion object {
        /** The Google TV Streamer on its HDR10 monitor, as probed (core/caps' fixture); match content: never. */
        val STREAMER = DisplayState(
            modes = listOf(
                DisplayModeInfo(40, 720, 480, 60.000004f, hdrTypes = listOf(2)),
                DisplayModeInfo(41, 720, 576, 50.0f, hdrTypes = listOf(2)),
                DisplayModeInfo(44, 1920, 1080, 60.000004f, hdrTypes = listOf(2)),
                DisplayModeInfo(45, 1920, 1080, 50.0f, hdrTypes = listOf(2)),
                DisplayModeInfo(46, 3840, 2160, 23.976f, hdrTypes = listOf(2)),
                DisplayModeInfo(47, 3840, 2160, 24.000002f, hdrTypes = listOf(2)),
                DisplayModeInfo(48, 3840, 2160, 25.0f, hdrTypes = listOf(2)),
                DisplayModeInfo(49, 3840, 2160, 29.97f, hdrTypes = listOf(2)),
                DisplayModeInfo(50, 3840, 2160, 30.000002f, hdrTypes = listOf(2)),
                DisplayModeInfo(51, 3840, 2160, 60.000004f, hdrTypes = listOf(2)),
                DisplayModeInfo(52, 3840, 2160, 50.0f, hdrTypes = listOf(2)),
                DisplayModeInfo(54, 1920, 1080, 59.94f, hdrTypes = listOf(2)),
                DisplayModeInfo(55, 3840, 2160, 59.94f, hdrTypes = listOf(2)),
            ),
            activeModeId = 51,
            hdrCapabilities = listOf(2),
            matchContent = MatchContent.Never,
        )

        /** The Galaxy Note20 Ultra at 1080p and 60 Hz, as probed; seamless only. */
        val NOTE20 = DisplayState(
            modes = listOf(
                DisplayModeInfo(3, 1080, 2316, 120.00001f, listOf(48.000004f, 60.000004f, 96.00001f)),
                DisplayModeInfo(4, 1080, 2316, 96.00001f, listOf(48.000004f, 60.000004f, 120.00001f)),
                DisplayModeInfo(5, 1080, 2316, 60.000004f, listOf(48.000004f, 96.00001f, 120.00001f)),
                DisplayModeInfo(6, 1080, 2316, 48.000004f, listOf(60.000004f, 96.00001f, 120.00001f)),
            ),
            activeModeId = 5,
            hdrCapabilities = listOf(2, 3, 4),
            matchContent = MatchContent.SeamlessOnly,
        )
    }
}
