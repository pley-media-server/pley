package app.pley.android.player.engine

import android.media.AudioManager
import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.exoplayer.ExoPlaybackException
import androidx.media3.exoplayer.audio.AudioSink
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class AudioRulesTest {
    private class Recorder : FocusTarget, FocusPort {
        val calls = mutableListOf<String>()
        var granted = true
        var listener: ((Int) -> Unit)? = null

        override fun pause() {
            calls += "pause"
        }

        override fun play() {
            calls += "play"
        }

        override fun setVolume(volume: Float) {
            calls += "volume $volume"
        }

        override fun request(onChange: (Int) -> Unit): Boolean {
            calls += "request"
            listener = onChange
            return granted
        }

        override fun abandon() {
            calls += "abandon"
        }
    }

    private val r = Recorder()
    private val rules = AudioFocusRules(r, r)

    @Test
    fun focusIsAskedForOnceBeforePlaying() {
        assertTrue(rules.beforePlay())
        assertTrue(rules.beforePlay())
        assertEquals(listOf("request"), r.calls)
    }

    @Test
    fun aRefusedFocusMeansNoPlaying() {
        r.granted = false
        assertFalse(rules.beforePlay())
    }

    @Test
    fun pcmDucksAndComesBack() {
        rules.beforePlay()
        rules.onFocusChange(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK)
        rules.onFocusChange(AudioManager.AUDIOFOCUS_GAIN)
        assertEquals(listOf("request", "volume 0.2", "volume 1.0"), r.calls)
    }

    @Test
    fun passthroughPausesInsteadOfDuckingAndResumes() {
        rules.passthrough = true
        rules.beforePlay()
        rules.onFocusChange(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK)
        rules.onFocusChange(AudioManager.AUDIOFOCUS_GAIN)
        assertEquals(listOf("request", "pause", "play"), r.calls)
    }

    @Test
    fun aTransientLossResumesUnlessThePersonPaused() {
        rules.beforePlay()
        rules.onFocusChange(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT)
        rules.onPausedByPerson()
        rules.onFocusChange(AudioManager.AUDIOFOCUS_GAIN)
        assertEquals(listOf("request", "pause"), r.calls)
    }

    @Test
    fun aLossForGoodPausesAndGivesTheFocusBack() {
        rules.beforePlay()
        rules.onFocusChange(AudioManager.AUDIOFOCUS_LOSS)
        rules.onFocusChange(AudioManager.AUDIOFOCUS_GAIN)
        assertEquals(listOf("request", "abandon", "pause"), r.calls)
        // Playing again asks again.
        rules.beforePlay()
        assertEquals("request", r.calls.last())
    }

    @Test
    fun aLiveChannelGoesQuietForACallAndIsHeardAgainAfter() {
        rules.live = true
        assertTrue(rules.beforePlay())
        rules.onFocusChange(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT)
        // Tuned again meanwhile (a channel changed, a retry): still quiet.
        assertTrue(rules.beforePlay())
        rules.onPausedByPerson()
        rules.onFocusChange(AudioManager.AUDIOFOCUS_GAIN)
        assertEquals(listOf("request", "volume 0.0", "volume 1.0"), r.calls)
    }

    @Test
    fun aLiveChannelLostForGoodStaysQuietUntilItIsTunedAgain() {
        rules.live = true
        rules.beforePlay()
        rules.onFocusChange(AudioManager.AUDIOFOCUS_LOSS)
        assertEquals(listOf("request", "abandon", "volume 0.0"), r.calls)
        assertTrue(rules.beforePlay())
        assertEquals(listOf("request", "abandon", "volume 0.0", "request", "volume 1.0"), r.calls)
    }

    @Test
    fun aLiveChannelPlaysQuietWhenTheFocusIsRefused() {
        rules.live = true
        r.granted = false
        assertTrue("no pause for the player to undo", rules.beforePlay())
        assertEquals(listOf("request", "volume 0.0"), r.calls)
        // A video after it isn't left quiet.
        rules.live = false
        assertEquals("volume 1.0", r.calls.last())
    }

    @Test
    fun releaseGivesTheFocusBack() {
        rules.release()
        assertEquals(emptyList<String>(), r.calls)
        rules.beforePlay()
        rules.release()
        assertEquals(listOf("request", "abandon"), r.calls)
    }

    private fun audioError(cause: Exception, code: Int): PlaybackException =
        ExoPlaybackException.createForRenderer(cause, "audio", 1, Format.Builder().setSampleMimeType(MimeTypes.AUDIO_E_AC3).build(), C.FORMAT_HANDLED, null, false, code)

    @Test
    fun aDeadAudioTrackIsStartedAgainAFewTimesAMinute() {
        var now = 0L
        val restarts = AudioRestarts({ now })
        val dead = audioError(AudioSink.WriteException(AudioRestarts.DEAD_OBJECT, Format.Builder().build(), false), PlaybackException.ERROR_CODE_AUDIO_TRACK_WRITE_FAILED)
        assertTrue(restarts.shouldRestart(dead))
        assertTrue(restarts.shouldRestart(dead))
        assertTrue(restarts.shouldRestart(dead))
        assertFalse("three a minute at most", restarts.shouldRestart(dead))
        now += AudioRestarts.WINDOW_MS + 1
        assertTrue(restarts.shouldRestart(dead))
    }

    @Test
    fun onlyAudioOutputFailuresRestart() {
        val restarts = AudioRestarts({ 0L })
        val init = audioError(IllegalStateException("init"), PlaybackException.ERROR_CODE_AUDIO_TRACK_INIT_FAILED)
        assertTrue(restarts.shouldRestart(init))
        val decoder = audioError(IllegalStateException("decoder"), PlaybackException.ERROR_CODE_DECODING_FAILED)
        assertFalse(restarts.shouldRestart(decoder))
        val network = PlaybackException("io", null, PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_FAILED)
        assertFalse(restarts.shouldRestart(network))
    }

    @Test
    fun encodedAudioPassesThrough() {
        assertTrue(PlaybackAnalytics.isPassthrough(C.ENCODING_E_AC3_JOC))
        assertTrue(PlaybackAnalytics.isPassthrough(C.ENCODING_DTS))
        assertFalse(PlaybackAnalytics.isPassthrough(C.ENCODING_PCM_16BIT))
        assertFalse(PlaybackAnalytics.isPassthrough(C.ENCODING_PCM_FLOAT))
        assertFalse(PlaybackAnalytics.isPassthrough(Format.NO_VALUE))
    }
}
