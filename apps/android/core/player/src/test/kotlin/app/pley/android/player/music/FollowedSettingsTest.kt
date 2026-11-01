package app.pley.android.player.music

import app.pley.android.caps.ModeSwitching
import app.pley.android.caps.Override
import app.pley.android.caps.PassthroughFormat
import app.pley.android.data.FrameRateMatching
import app.pley.android.data.Passthrough
import app.pley.android.data.Settings
import app.pley.android.player.capsSettings
import app.pley.android.player.engine.RefreshRateMatching
import app.pley.android.player.engineSettings
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** What the engines and the probe make of the app's settings. */
class FollowedSettingsTest {
    @Test
    fun aTvsRefreshRateChoiceGoesToTheEngine() {
        assertEquals(RefreshRateMatching.Auto, engineSettings(Settings(), tv = true).refreshRate)
        assertEquals(RefreshRateMatching.Always, engineSettings(Settings(frameRateMatching = FrameRateMatching.Always), tv = true).refreshRate)
        assertEquals(RefreshRateMatching.Off, engineSettings(Settings(frameRateMatching = FrameRateMatching.Off), tv = true).refreshRate)
        // A phone's choice on a TV is the TV's default.
        assertEquals(RefreshRateMatching.Auto, engineSettings(Settings(frameRateMatching = FrameRateMatching.Seamless), tv = true).refreshRate)
        assertEquals(RefreshRateMatching.Off, engineSettings(Settings(frameRateMatching = FrameRateMatching.Off), tv = false).refreshRate)
    }

    @Test
    fun tunnelingIsATvsOnly() {
        assertTrue(engineSettings(Settings(tunneling = true), tv = true).tunneling)
        assertFalse(engineSettings(Settings(tunneling = true), tv = false).tunneling)
    }

    @Test
    fun passthroughChoicesBecomeTheProbesOverrides() {
        val caps = capsSettings(Settings(passthrough = mapOf("eac3-joc" to Passthrough.On, "dts" to Passthrough.Off, "ac3" to Passthrough.Auto)), tv = true)
        assertEquals(mapOf(PassthroughFormat.Atmos to Override.On, PassthroughFormat.Dts to Override.Off), caps.passthrough)
        assertTrue(capsSettings(Settings(allowSoftwareVideo = true), tv = false).allowSoftwareDecoders)
    }

    @Test
    fun aTvsRefreshRateChoiceSaysWhichModesTheProbeCounts() {
        assertEquals(ModeSwitching.Auto, capsSettings(Settings(), tv = true).modeSwitching)
        assertEquals(ModeSwitching.Always, capsSettings(Settings(frameRateMatching = FrameRateMatching.Always), tv = true).modeSwitching)
        assertEquals(ModeSwitching.Off, capsSettings(Settings(frameRateMatching = FrameRateMatching.Off), tv = true).modeSwitching)
    }
}
