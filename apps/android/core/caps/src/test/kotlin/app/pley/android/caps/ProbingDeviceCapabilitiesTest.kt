package app.pley.android.caps

import app.pley.android.model.DynamicRange
import app.pley.android.testing.caps.TestDevices
import app.pley.android.testing.caps.TestDisplays
import app.pley.android.testing.caps.TestQuirks
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

@OptIn(ExperimentalCoroutinesApi::class)
class ProbingDeviceCapabilitiesTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val device = TestDevices.fireTvStick4kMax2023

    private class Sources(var input: ProbeInput) {
        var codecReads = 0
        var onChange: (() -> Unit)? = null
        var stopped = false

        val codecs = CodecInventory { codecReads++; input.decoders }
        val display = DisplayInfo { input.display }
        val audio = AudioInfo { input.audio }
        val changes = object : ChangeSource {
            override fun start(onChange: () -> Unit) {
                this@Sources.onChange = onChange
            }

            override fun stop() {
                stopped = true
            }
        }
    }

    private fun TestScope.probe(sources: Sources, cache: File? = null, language: () -> String = { "" }) = ProbingDeviceCapabilities(
        device = { sources.input.device },
        codecs = sources.codecs,
        display = sources.display,
        audio = sources.audio,
        builtInQuirks = { TestQuirks.shipped },
        changes = sources.changes,
        cache = cache?.let { CapsCache(it) },
        installedAt = 1,
        describe = { listOf(language() + "${it.report.device?.model}: ${it.report.videoCodecs.joinToString()}") },
        language = language,
        dispatcher = StandardTestDispatcher(testScheduler),
        scope = backgroundScope,
    )

    @Test
    fun aSnapshotAskedForInAnotherLanguageIsDescribedInIt() = runTest {
        val sources = Sources(device)
        var language = "en "
        val caps = probe(sources, language = { language })
        assertEquals(listOf("en AFTKRT: h264, hevc, vp9, av1"), caps.warmUp().lines)
        // Asked for again in the same language: the same snapshot.
        val first = caps.snapshot.value
        assertTrue(first === caps.warmUp())
        language = "fr "
        val again = caps.warmUp()
        assertEquals(listOf("fr AFTKRT: h264, hevc, vp9, av1"), again.lines)
        assertEquals(again, caps.snapshot.value)
        assertEquals(first!!.report, again.report)
        assertEquals("described again, not probed again", 1, sources.codecReads)
    }

    @Test
    fun warmUpProbesOnceAndWatches() = runTest {
        val sources = Sources(device)
        val caps = probe(sources)
        assertNull(caps.snapshot.value)
        val first = caps.warmUp()
        assertEquals(1, sources.codecReads)
        assertEquals(first, caps.snapshot.value)
        assertEquals(listOf("AFTKRT: h264, hevc, vp9, av1"), first.lines)
        runCurrent()
        assertNotNull("watching the screen and the output", sources.onChange)
        assertEquals(first, caps.warmUp())
        assertEquals(1, sources.codecReads)
        caps.close()
        assertTrue(sources.stopped)
    }

    @Test
    fun aChangeProbesAgainOnceThingsSettle() = runTest {
        val sources = Sources(device)
        val caps = probe(sources)
        caps.warmUp()
        // The TV is swapped for an HDR10-only one: the screen changes twice in a row.
        sources.input = device.copy(display = TestDisplays.tv(hdr = listOf(TestDisplays.HDR10)))
        sources.onChange!!()
        advanceTimeBy(100)
        sources.onChange!!()
        advanceTimeBy(200)
        runCurrent()
        assertTrue("not settled yet", caps.snapshot.value!!.report.dolbyVision)
        advanceTimeBy(150)
        runCurrent()
        val now = caps.snapshot.value!!
        assertFalse(now.report.dolbyVision)
        assertEquals(listOf(DynamicRange.Hdr10), now.report.hdrTypes)
        assertEquals("decoders are read once", 1, sources.codecReads)
    }

    @Test
    fun settingsAndServerQuirksChangeTheReport() = runTest {
        val sources = Sources(device)
        val caps = probe(sources)
        caps.warmUp()
        caps.updateSettings(CapsSettings(passthrough = mapOf(PassthroughFormat.TrueHd to Override.Off)))
        advanceTimeBy(400)
        runCurrent()
        assertEquals(listOf("ac3", "eac3", "dts"), caps.snapshot.value!!.report.audioPassthrough)
        caps.updateQuirks(QuirkTable(quirks = listOf(Quirk(id = "test", models = listOf("AFTKRT"), remove = CapsChange(videoCodecs = listOf("av1"))))))
        advanceTimeBy(400)
        runCurrent()
        assertEquals(listOf("h264", "hevc", "vp9"), caps.snapshot.value!!.report.videoCodecs)
        assertEquals(1, sources.codecReads)
    }

    @Test
    fun theSavedProbeIsUsedUntilSomethingChanges() = runTest {
        val file = File(folder.root, "caps/snapshot.json")
        val first = probe(Sources(device), file).warmUp()
        runCurrent()
        assertTrue(file.isFile)

        val again = Sources(device)
        assertEquals("same build, screen and output: no probe", first, probe(again, file).warmUp())
        assertEquals(0, again.codecReads)

        val otherScreen = Sources(device.copy(display = TestDisplays.tv(hdr = listOf(TestDisplays.HDR10))))
        assertFalse(probe(otherScreen, file).warmUp().report.dolbyVision)
        assertEquals("the decoders are kept for the same system build", 0, otherScreen.codecReads)
        runCurrent()

        val update = Sources(device.copy(device = device.device.copy(fingerprint = "Amazon/kara/kara:12")))
        probe(update, file).warmUp()
        assertEquals("a system update reads the decoders again", 1, update.codecReads)
        runCurrent()

        file.writeText("{ not json")
        val broken = Sources(device)
        assertEquals(first.report, probe(broken, file).warmUp().report)
        assertEquals(1, broken.codecReads)
    }
}
