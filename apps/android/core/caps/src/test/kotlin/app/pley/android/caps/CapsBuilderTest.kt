package app.pley.android.caps

import androidx.media3.common.C
import app.pley.android.model.DynamicRange
import app.pley.android.model.caps.DeviceForm
import app.pley.android.model.caps.DolbyVisionCaps
import app.pley.android.model.caps.VideoDecoderCaps
import app.pley.android.testing.caps.TestAudio
import app.pley.android.testing.caps.TestDevices
import app.pley.android.testing.caps.TestQuirks
import app.pley.android.testing.caps.capsOf
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CapsBuilderTest {
    private fun entries(snapshot: CapsSnapshot, codec: String): List<VideoDecoderCaps> =
        snapshot.report.video.orEmpty().filter { it.codec == codec }

    @Test
    fun tvsNeverListSoftwareVideoDecoders() {
        TestDevices.all.filter { it.value.device.isTv }.forEach { (name, device) ->
            val caps = capsOf(device)
            assertTrue("$name lists a software decoder", caps.report.video.orEmpty().all { it.hardware })
            assertTrue(name, caps.details.policy.tv)
        }
        assertTrue("the Shield has no AV1 hardware", entries(capsOf(TestDevices.shield), "av1").isEmpty())
        assertFalse("nor does the 1080p Fire TV stick", "av1" in capsOf(TestDevices.fireTvStickLite).report.videoCodecs)
        assertEquals(
            "the Chromecast HD decodes AV1 in hardware up to 1080p",
            listOf(VideoDecoderCaps("av1", true, listOf("Main"), 1920, 1080, 60.0, 10, 9)),
            entries(capsOf(TestDevices.chromecastHd), "av1"),
        )
    }

    @Test
    fun theFloorStickStopsAt1080pHevcLevel41() {
        val caps = capsOf(TestDevices.fireTvStickLite)
        assertEquals(
            listOf(VideoDecoderCaps("hevc", true, listOf("Main", "Main 10"), 1920, 1080, 60.0, 10, 123)),
            entries(caps, "hevc"),
        )
        assertEquals(listOf("h264", "hevc", "vp9"), caps.report.videoCodecs)
        assertEquals(emptyList<DolbyVisionCaps>(), caps.report.dolbyVisionProfiles)
        assertFalse(caps.report.dolbyVision)
        assertTrue("HDR10 screen and a Main 10 decoder", caps.report.hdr)
        assertEquals("no decoder lists HDR10+", listOf(DynamicRange.Hdr10, DynamicRange.Hlg), caps.report.hdrTypes)
        assertEquals("low", caps.report.device?.tier)
        assertEquals("T0", caps.details.tier)
        assertTrue(caps.report.device!!.fireTv)
        assertTrue("Amazon asks for async queueing", caps.details.forceAsyncQueueing)
    }

    @Test
    fun phonesUseSoftwareOnlyUpTo1080p30EightBitSdr() {
        val caps = capsOf(TestDevices.sdrPhone)
        assertEquals(
            listOf(VideoDecoderCaps("av1", false, listOf("Main"), 1920, 1080, 30.0, 8, 15)),
            entries(caps, "av1"),
        )
        assertEquals(
            "4K at 30, and 1080p faster",
            listOf(3840 to 30.0, 1920 to 60.0),
            entries(caps, "h264").map { it.maxWidth to it.maxFps },
        )
        assertTrue("hardware wins over software", entries(caps, "hevc").all { it.hardware })
        assertEquals("the debug switch lifts the limit", 60.0, entries(capsOf(TestDevices.sdrPhone, CapsSettings(allowSoftwareDecoders = true)), "av1").single().maxFps)
        val policy = caps.details.policy
        assertTrue(policy.allows(hardware = false, width = 1920, height = 1080, fps = 29.97, bitDepth = 8, hdr = false))
        assertTrue("portrait counts too", policy.allows(hardware = false, width = 1080, height = 1920, fps = 30.0, bitDepth = 8, hdr = false))
        assertFalse(policy.allows(hardware = false, width = 1920, height = 1080, fps = 60.0, bitDepth = 8, hdr = false))
        assertFalse(policy.allows(hardware = false, width = 1920, height = 1080, fps = 24.0, bitDepth = 10, hdr = false))
        assertFalse(policy.allows(hardware = false, width = 1280, height = 720, fps = 24.0, bitDepth = 10, hdr = true))
        assertFalse(policy.allows(hardware = false, width = 2560, height = 1440, fps = 24.0, bitDepth = 8, hdr = false))
        assertFalse("TVs: hardware only", capsOf(TestDevices.shield).details.policy.allows(false, 640, 360, 24.0, 8, false))
        assertTrue(capsOf(TestDevices.shield, CapsSettings(allowSoftwareDecoders = true)).details.policy.allows(false, 3840, 2160, 60.0, 10, true))
    }

    @Test
    fun hdrNeedsTheScreenAndA10BitHardwareDecoder() {
        val sdr = capsOf(TestDevices.sdrPhone)
        assertFalse(sdr.report.hdr)
        assertEquals(emptyList<DynamicRange>(), sdr.report.hdrTypes)
        val hdr = capsOf(TestDevices.hdr10Phone)
        assertTrue(hdr.report.hdr)
        assertEquals(listOf(DynamicRange.Hdr10, DynamicRange.Hlg), hdr.report.hdrTypes)
        val noTenBit = TestDevices.hdr10Phone.copy(decoders = TestDevices.hdr10Phone.decoders.filter { !it.hardware || it.mime == Codecs.AVC })
        assertFalse("software 10-bit decoders don't count", capsOf(noTenBit).report.hdr)
        assertEquals("but the screen is still reported", listOf(DynamicRange.Hdr10, DynamicRange.Hlg), capsOf(noTenBit).details.screenHdrTypes)
    }

    @Test
    fun dolbyVisionProfilesComeFromHardwareDecoders() {
        val phone = capsOf(TestDevices.dvPhone)
        assertEquals(listOf(DolbyVisionCaps(5, 7), DolbyVisionCaps(8, 7)), phone.report.dolbyVisionProfiles)
        assertTrue(phone.report.dolbyVision)
        assertEquals(listOf(DynamicRange.Hdr10, DynamicRange.Hdr10Plus, DynamicRange.Hlg, DynamicRange.DolbyVision), phone.report.hdrTypes)

        val max = capsOf(TestDevices.fireTvStick4kMax2023)
        assertEquals(listOf(5, 8, 9, 10), max.report.dolbyVisionProfiles!!.map { it.profile })
        assertEquals(listOf(9, 9, 5, 9), max.report.dolbyVisionProfiles!!.map { it.maxLevel })
        assertTrue(max.details.dvHdr10PlusHybridsBroken)

        val onSdr = capsOf(TestDevices.dvPhone.copy(display = TestDevices.sdrPhone.display))
        // Listed whatever the screen: the server then says the screen is what doesn't show it.
        assertEquals(listOf(5, 8), onSdr.report.dolbyVisionProfiles!!.map { it.profile })
        assertFalse(DynamicRange.DolbyVision in onSdr.report.hdrTypes.orEmpty())
        assertFalse(onSdr.report.dolbyVision)
        assertEquals("still known for Diagnostics", listOf(5, 8), onSdr.details.dolbyVisionProfiles.map { it.profile })

        val noP5 = TestDevices.dvPhone.copy(decoders = TestDevices.dvPhone.decoders.filter { it.mime != Codecs.DOLBY_VISION } + app.pley.android.testing.caps.TestDecoders.dolbyVision("dv", mapOf(8 to 7)))
        assertFalse("version 1 means profile 5", capsOf(noP5).report.dolbyVision)
        assertEquals(listOf(8), capsOf(noP5).report.dolbyVisionProfiles!!.map { it.profile })
    }

    @Test
    fun quirksAddAndRemove() {
        val shield = capsOf(TestDevices.shield)
        assertEquals("P7 added, level unknown", listOf(DolbyVisionCaps(4, 9), DolbyVisionCaps(5, 9), DolbyVisionCaps(7, null), DolbyVisionCaps(8, 9)), shield.report.dolbyVisionProfiles)
        assertEquals(listOf("shield-dv-p7"), shield.details.quirks)

        val streamer = capsOf(TestDevices.googleTvStreamer)
        assertEquals(listOf("ac3", "eac3", "dts"), streamer.report.audioPassthrough)
        assertEquals(listOf(C.ENCODING_AC3, C.ENCODING_E_AC3, C.ENCODING_DTS, C.ENCODING_E_AC3_JOC), streamer.details.passthroughEncodings)
        assertEquals(listOf("google-tv-streamer-dv-hdr10plus-hybrids", "google-tv-lossless-passthrough"), streamer.details.quirks)
        assertTrue(streamer.details.dvHdr10PlusHybridsBroken)
        assertFalse("not a Fire TV", streamer.details.forceAsyncQueueing)

        val max = capsOf(TestDevices.fireTvStick4kMax2023)
        assertEquals("Fire TV keeps lossless passthrough", listOf("ac3", "eac3", "truehd", "dts"), max.report.audioPassthrough)
        assertFalse("no model is on the tunneling list yet", max.details.tunnelingAllowed)
    }

    @Test
    fun serverTablesReplaceAddAndSwitchOffQuirks() {
        val server = QuirkTable(
            version = 2,
            quirks = listOf(
                Quirk(id = "shield-dv-p7", disabled = true),
                Quirk(id = "fire-tv-async", models = listOf("AFTKRT"), flags = QuirkFlags(asyncQueueing = false, tunneling = true)),
                Quirk(id = "no-av1", models = listOf("Chromecast HD"), remove = CapsChange(videoCodecs = listOf("av1"), hdrTypes = listOf(DynamicRange.Hdr10Plus))),
            ),
        )
        val table = TestQuirks.shipped.overriddenBy(server)
        assertFalse(7 in CapsBuilder.build(TestDevices.shield, table).report.dolbyVisionProfiles!!.map { it.profile })
        val max = CapsBuilder.build(TestDevices.fireTvStick4kMax2023, table).details
        assertFalse("the kill switch", max.forceAsyncQueueing)
        assertTrue(max.tunnelingAllowed)
        assertTrue("the shipped quirk still applies", max.dvHdr10PlusHybridsBroken)
        val ccwgtv = CapsBuilder.build(TestDevices.chromecastHd, table)
        assertFalse("av1" in ccwgtv.report.videoCodecs)
        assertFalse(DynamicRange.Hdr10Plus in ccwgtv.details.screenHdrTypes)
    }

    @Test
    fun passthroughOverridesWinOverTheDeviceAndTheQuirks() {
        val streamer = TestDevices.googleTvStreamer
        val on = capsOf(streamer, CapsSettings(passthrough = mapOf(PassthroughFormat.TrueHd to Override.On, PassthroughFormat.Atmos to Override.Off)))
        assertTrue(C.ENCODING_DOLBY_TRUEHD in on.details.passthroughEncodings)
        assertFalse(C.ENCODING_E_AC3_JOC in on.details.passthroughEncodings)
        assertEquals(listOf("ac3", "eac3", "truehd", "dts"), on.report.audioPassthrough)

        val off = capsOf(streamer, CapsSettings(passthrough = mapOf(PassthroughFormat.DolbyDigital to Override.Off, PassthroughFormat.Dts to Override.Off)))
        assertEquals(listOf("eac3"), off.report.audioPassthrough)
        assertEquals("decoded or passed through", listOf("aac", "mp3", "opus", "vorbis", "flac", "ac3", "eac3", "pcm_s16le", "pcm_s24le"), off.report.audioCodecs)

        val bluetooth = capsOf(streamer.copy(audio = streamer.audio.copy(route = AudioState.ROUTE_BLUETOOTH)), CapsSettings(passthrough = mapOf(PassthroughFormat.TrueHd to Override.On)))
        assertEquals("Bluetooth gets PCM", emptyList<String>(), bluetooth.report.audioPassthrough)
        assertEquals(2, bluetooth.report.audioChannels)
    }

    @Test
    fun audioChannelsFollowTheOutput() {
        assertEquals(2, capsOf(TestDevices.sdrPhone).report.audioChannels)
        assertEquals(8, capsOf(TestDevices.shield).report.audioChannels)
        assertEquals("a TV's own speakers", 2, capsOf(TestDevices.shield.copy(audio = TestAudio.speaker)).report.audioChannels)
    }

    @Test
    fun displaysReportTheirModeAndTheRatesTheyReach() {
        val tv = capsOf(TestDevices.fireTvStick4kMax2023).report.display!!
        assertEquals(3840 to 2160, tv.width to tv.height)
        assertEquals(listOf(23.976f, 24f, 25f, 29.97f, 30f, 50f, 59.94f, 60f), tv.refreshRates)
        val phone = capsOf(TestDevices.sdrPhone).report.display!!
        assertEquals(listOf(60f, 90f, 120f), phone.refreshRates)
        assertEquals(60f, CapsBuilder.roundRate(60.000004f))
        assertEquals(23.976f, CapsBuilder.roundRate(23.976025f))
        assertNull(capsOf(TestDevices.sdrPhone.copy(display = null)).report.display)
    }

    @Test
    fun screenHdrTypesPerModeOnAndroid14() {
        val modes = listOf(
            DisplayModeInfo(1, 3840, 2160, 60f, hdrTypes = listOf(2)),
            DisplayModeInfo(2, 3840, 2160, 24f, hdrTypes = listOf(1, 2)),
        )
        val state = DisplayState(modes, activeModeId = 1, hdrCapabilities = listOf(2))
        assertEquals("a TV may switch to the mode that shows Dolby Vision", listOf(DynamicRange.Hdr10, DynamicRange.DolbyVision), CapsBuilder.screenHdrTypes(state, tv = true))
        assertEquals("a phone shows what its mode shows", listOf(DynamicRange.Hdr10), CapsBuilder.screenHdrTypes(state, tv = false))
        val older = DisplayState(modes.map { it.copy(hdrTypes = null) }, activeModeId = 1, hdrCapabilities = listOf(2, 3))
        assertEquals(listOf(DynamicRange.Hdr10, DynamicRange.Hlg), CapsBuilder.screenHdrTypes(older, tv = true))
        val empty = DisplayState(modes.map { it.copy(hdrTypes = emptyList()) }, activeModeId = 1, hdrCapabilities = listOf(2))
        assertEquals("modes that list nothing fall back", listOf(DynamicRange.Hdr10), CapsBuilder.screenHdrTypes(empty, tv = true))
    }

    /** Dolby Vision at 24 Hz only: a TV promises it only where its player can switch to 24 Hz. */
    @Test
    fun screenHdrTypesCountTheModesAVideoPlaysIn() {
        val modes = listOf(
            DisplayModeInfo(1, 3840, 2160, 60f, hdrTypes = listOf(2)),
            DisplayModeInfo(2, 3840, 2160, 24f, hdrTypes = listOf(1, 2)),
            DisplayModeInfo(3, 1920, 1080, 24f, hdrTypes = listOf(1, 2)),
        )
        val dv = listOf(DynamicRange.Hdr10, DynamicRange.DolbyVision)
        val hdr10 = listOf(DynamicRange.Hdr10)
        fun types(match: MatchContent, switching: ModeSwitching = ModeSwitching.Auto, seamless: List<Float> = emptyList()): List<DynamicRange> {
            val state = DisplayState(
                modes.map { if (it.id == 1) it.copy(alternativeRefreshRates = seamless) else it },
                activeModeId = 1,
                hdrCapabilities = listOf(2),
                matchContent = match,
            )
            return CapsBuilder.screenHdrTypes(state, tv = true, switching = switching)
        }
        assertEquals(dv, types(MatchContent.Always))
        assertEquals("the device never switches", hdr10, types(MatchContent.Never))
        assertEquals("unless pley's own setting says Always", dv, types(MatchContent.Never, ModeSwitching.Always))
        assertEquals("pley's setting is Off", hdr10, types(MatchContent.Always, ModeSwitching.Off))
        assertEquals("seamless only, and 24 Hz isn't", hdr10, types(MatchContent.SeamlessOnly))
        assertEquals(dv, types(MatchContent.SeamlessOnly, seamless = listOf(24f)))
    }

    /** The person turned Dolby Vision off: the mode lists it, getHdrCapabilities doesn't, and neither does Media3. */
    @Test
    fun hdrTypesTurnedOffAreLeftOut() {
        val modes = listOf(
            DisplayModeInfo(1, 3840, 2160, 60f, hdrTypes = listOf(1, 2)),
            DisplayModeInfo(2, 3840, 2160, 24f, hdrTypes = listOf(1, 2)),
        )
        val state = DisplayState(modes, activeModeId = 1, hdrCapabilities = listOf(2), matchContent = MatchContent.Always)
        assertEquals(setOf(1), state.turnedOffHdrTypes)
        assertEquals(listOf(DynamicRange.Hdr10), CapsBuilder.screenHdrTypes(state, tv = true))
        assertEquals(listOf(DynamicRange.Hdr10), CapsBuilder.screenHdrTypes(state, tv = false))
        val on = state.copy(hdrCapabilities = listOf(1, 2))
        assertEquals(listOf(DynamicRange.Hdr10, DynamicRange.DolbyVision), CapsBuilder.screenHdrTypes(on, tv = true))
    }

    @Test
    fun hdrTypesTurnedOffAreRememberedFromTheModesThatListThem() {
        val modes = listOf(
            DisplayModeInfo(1, 3840, 2160, 60f, hdrTypes = listOf(2)),
            DisplayModeInfo(2, 3840, 2160, 24f, hdrTypes = listOf(1, 2)),
        )
        val home = DisplayState(modes, activeModeId = 1, hdrCapabilities = listOf(2))
        // The 60 Hz mode doesn't list Dolby Vision: what was seen at 24 Hz stays.
        assertEquals(setOf(1), home.turnedOffHdrTypes(remembered = setOf(1)))
        assertEquals(emptySet<Int>(), home.turnedOffHdrTypes(remembered = emptySet()))
        // At 24 Hz the display tells: off, or on again.
        val film = home.copy(activeModeId = 2)
        assertEquals(setOf(1), film.turnedOffHdrTypes(remembered = emptySet()))
        assertEquals(emptySet<Int>(), film.copy(hdrCapabilities = listOf(1, 2)).turnedOffHdrTypes(remembered = setOf(1)))
        // Before Android 14 modes list nothing: as remembered.
        val old = DisplayState(modes.map { it.copy(hdrTypes = null) }, activeModeId = 2, hdrCapabilities = listOf(2))
        assertEquals(setOf(1), old.turnedOffHdrTypes(remembered = setOf(1)))
    }

    @Test
    fun aVideoTheScreenDoesntShowAsDolbyVisionIsAskedForWithoutIt() {
        val streamer = TestDevices.googleTvStreamer
        val dvScreen = streamer.display!!.copy(modes = streamer.display!!.modes.map { it.copy(hdrTypes = listOf(1, 2, 3)) }, hdrCapabilities = listOf(1, 2, 3))
        val snapshot = capsOf(streamer.copy(display = dvScreen))
        assertTrue(DynamicRange.DolbyVision in snapshot.report.hdrTypes.orEmpty())
        val without = snapshot.withoutDolbyVisionScreen()
        assertFalse(DynamicRange.DolbyVision in without.report.hdrTypes.orEmpty())
        assertFalse(DynamicRange.DolbyVision in without.details.screenHdrTypes)
        assertFalse(without.report.dolbyVision)
        assertEquals("the decoders' profiles stay", snapshot.report.dolbyVisionProfiles, without.report.dolbyVisionProfiles)
        assertEquals(snapshot.report.hdrTypes.orEmpty() - DynamicRange.DolbyVision, without.report.hdrTypes)
    }

    @Test
    fun devicesFallIntoTiers() {
        val tiers = TestDevices.all.mapValues { capsOf(it.value).report.device!!.tier }
        assertEquals("low", tiers["fireTvStickLite"])
        assertEquals("low", tiers["chromecastHd"])
        assertEquals("mid", tiers["fireTvStick4kMax2023"])
        assertEquals("high", tiers["shield"])
        assertEquals("high", tiers["googleTvStreamer"])
        assertEquals("high", tiers["sdrPhone"])
        assertEquals(DeviceForm.Tv, capsOf(TestDevices.shield).report.device!!.form)
        assertEquals(DeviceForm.Phone, capsOf(TestDevices.sdrPhone).report.device!!.form)
    }

    @Test
    fun versionOneFieldsAreFilledForTodaysServer() {
        val caps = capsOf(TestDevices.fireTvStick4kMax2023).report
        assertEquals(listOf("h264", "hevc", "vp9", "av1"), caps.videoCodecs)
        assertTrue("mkv" in caps.containers)
        assertTrue(caps.hdr)
        assertTrue(caps.dolbyVision)
        assertEquals(listOf("aac", "mp3", "opus", "vorbis", "flac", "ac3", "eac3", "pcm_s16le", "pcm_s24le", "truehd", "dts"), caps.audioCodecs)
        assertEquals(2, caps.capsVersion)
        assertEquals(true, caps.audioTrackSwitching)
    }
}
