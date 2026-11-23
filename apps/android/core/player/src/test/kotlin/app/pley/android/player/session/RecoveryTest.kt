package app.pley.android.player.session

import app.pley.android.model.PlaybackMethod
import app.pley.android.model.PlaybackMode
import app.pley.android.player.session.Recovery.Action
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The recovery ladder's transitions and budgets. */
class RecoveryTest {
    private val unsupportedAudio = EngineError.Unsupported(audio = true, container = false, detail = "audio")
    private val unsupportedContainer = EngineError.Unsupported(audio = false, container = true, detail = "container")
    private val unsupportedVideo = EngineError.Unsupported(audio = false, container = false, detail = "video")

    @Test
    fun `direct play falls back to a remux for H_264 and HEVC when only the audio or container failed, then a transcode`() {
        val r = Recovery()
        assertEquals(Action.NewSession(PlaybackMode.Remux, FallbackReason.Unplayable), r.decide(unsupportedAudio, PlaybackMethod.Direct, "hevc"))
        assertEquals(PlaybackMode.Remux, r.fallbackMode)
        // The remux fails too: a transcode, once.
        assertEquals(Action.NewSession(PlaybackMode.Transcode, FallbackReason.Unplayable), r.decide(unsupportedAudio, PlaybackMethod.Remux, "hevc"))
        assertEquals(Action.Fail(Failure.Unplayable, "audio"), r.decide(unsupportedAudio, PlaybackMethod.Transcode, "hevc"))
    }

    @Test
    fun `audio a decoder fails on is refused at once, in the same mode, once per codec`() {
        val r = Recovery()
        assertEquals(Action.NewSession(null, FallbackReason.AudioUnplayable), r.decide(unsupportedAudio, PlaybackMethod.Direct, "hevc", "EAC3"))
        assertEquals(setOf("eac3"), r.refusedAudio)
        assertNull(r.fallbackMode)
        assertEquals(FallbackReason.AudioUnplayable, r.fallbackReason)
        assertTrue(r.leftOriginal)
        // The server converted it to AC-3, which fails too: refused as well.
        assertEquals(Action.NewSession(null, FallbackReason.AudioUnplayable), r.decide(unsupportedAudio, PlaybackMethod.Remux, "hevc", "ac3"))
        assertEquals(setOf("eac3", "ac3"), r.refusedAudio)
        // A codec refused already, AAC, or none known: the ladder as before.
        assertEquals(Action.NewSession(PlaybackMode.Transcode, FallbackReason.Unplayable), r.decide(unsupportedAudio, PlaybackMethod.Remux, "hevc", "eac3"))
        assertEquals(Action.Fail(Failure.Unplayable, "audio"), r.decide(unsupportedAudio, PlaybackMethod.Transcode, "hevc", "aac"))
        assertEquals(setOf("eac3", "ac3"), r.refusedAudio)
        // A video failure refuses nothing, whatever the audio.
        val v = Recovery()
        assertEquals(PlaybackMode.Transcode, (v.decide(unsupportedVideo, PlaybackMethod.Direct, "h264", "eac3") as Action.NewSession).mode)
        assertTrue(v.refusedAudio.isEmpty())
    }

    @Test
    fun `a late error naming audio refused already gets the session again as it was, once`() {
        val decoder = EngineError.Unsupported(audio = true, container = false, detail = "decoder", audioCodec = "eac3")
        val r = Recovery()
        assertEquals(Action.NewSession(null, FallbackReason.AudioUnplayable), r.decide(decoder, PlaybackMethod.Direct, "h264", "eac3"))
        // The renderer's error for the same track, once the session that converts it is loaded.
        assertEquals(Action.NewSession(null, null), r.decide(decoder, PlaybackMethod.Remux, "h264"))
        assertEquals(FallbackReason.AudioUnplayable, r.fallbackReason)
        assertNull(r.fallbackMode)
        // Once per codec: then the ladder.
        assertEquals(Action.NewSession(PlaybackMode.Transcode, FallbackReason.Unplayable), r.decide(decoder, PlaybackMethod.Remux, "h264"))
    }

    @Test
    fun `a refusal keeps a fallback's mode`() {
        val r = Recovery()
        r.decide(unsupportedContainer, PlaybackMethod.Direct, "h264")
        assertEquals(Action.NewSession(PlaybackMode.Remux, FallbackReason.AudioUnplayable), r.decide(unsupportedAudio, PlaybackMethod.Remux, "h264", "truehd"))
        assertEquals(PlaybackMode.Remux, r.fallbackMode)
    }

    @Test
    fun `an audio output that fails gets a new session as it was, and refuses the codec the second time`() {
        val output = EngineError.Unsupported(audio = true, container = false, detail = "output", audioCodec = "eac3", output = true)
        val r = Recovery()
        assertEquals(Action.NewSession(null, null), r.decide(output, PlaybackMethod.Direct, "hevc", "eac3"))
        assertTrue(r.refusedAudio.isEmpty())
        assertNull(r.fallbackMode)
        assertEquals(Action.NewSession(null, FallbackReason.AudioUnplayable), r.decide(output, PlaybackMethod.Direct, "hevc", "eac3"))
        assertEquals(setOf("eac3"), r.refusedAudio)
        // In a transcode too, where the ladder has no step left.
        val t = Recovery()
        t.decide(unsupportedVideo, PlaybackMethod.Direct, "vp9")
        assertEquals(Action.NewSession(PlaybackMode.Transcode, null), t.decide(output, PlaybackMethod.Transcode, "vp9", "eac3"))
        assertEquals(Action.NewSession(PlaybackMode.Transcode, FallbackReason.AudioUnplayable), t.decide(output, PlaybackMethod.Transcode, "vp9", "eac3"))
        // An unknown codec: the ladder as before.
        assertEquals(PlaybackMode.Remux, (Recovery().decide(output, PlaybackMethod.Direct, "hevc") as Action.NewSession).mode)
    }

    @Test
    fun `a new item forgets the refused audio, Retry and another quality don't`() {
        val r = Recovery()
        r.decide(unsupportedAudio, PlaybackMethod.Direct, "h264", "eac3")
        r.retry()
        r.qualityChanged()
        assertEquals(setOf("eac3"), r.refusedAudio)
        assertTrue(r.leftOriginal)
        r.newItem()
        assertTrue(r.refusedAudio.isEmpty())
        assertEquals(false, r.leftOriginal)
        val output = EngineError.Unsupported(audio = true, container = false, detail = "output", audioCodec = "eac3", output = true)
        assertEquals(Action.NewSession(null, null), r.decide(output, PlaybackMethod.Direct, "h264", "eac3"))
    }

    @Test
    fun `a container failure of H_264 remuxes, of other video transcodes, as does a video failure`() {
        assertEquals(PlaybackMode.Remux, (Recovery().decide(unsupportedContainer, PlaybackMethod.Direct, "h264") as Action.NewSession).mode)
        assertEquals(PlaybackMode.Transcode, (Recovery().decide(unsupportedContainer, PlaybackMethod.Direct, "vp9") as Action.NewSession).mode)
        assertEquals(PlaybackMode.Transcode, (Recovery().decide(unsupportedVideo, PlaybackMethod.Direct, "h264") as Action.NewSession).mode)
    }

    @Test
    fun `each fallback happens once`() {
        val r = Recovery()
        r.decide(unsupportedVideo, PlaybackMethod.Direct, "h264")
        assertTrue(r.decide(unsupportedVideo, PlaybackMethod.Direct, "h264") is Action.Fail)
        val s = Recovery()
        s.decide(unsupportedAudio, PlaybackMethod.Direct, "h264")
        s.decide(unsupportedAudio, PlaybackMethod.Direct, "h264")
        assertTrue(s.decide(unsupportedAudio, PlaybackMethod.Direct, "h264") is Action.Fail)
    }

    @Test
    fun `expired sessions and network errors get three new sessions each`() {
        val r = Recovery()
        repeat(3) { assertEquals(Action.NewSession(null, null), r.decide(EngineError.Http(404, "x"), PlaybackMethod.Remux, "h264")) }
        assertEquals(Action.Fail(Failure.Stopped, "x"), r.decide(EngineError.Http(410, "x"), PlaybackMethod.Remux, "h264"))
        repeat(3) { assertEquals(Action.NewSession(null, null), r.decide(EngineError.Network("n"), PlaybackMethod.Remux, "h264")) }
        assertEquals(Action.Fail(Failure.Network, "n"), r.decide(EngineError.Http(500, "n"), PlaybackMethod.Remux, "h264"))
        assertTrue(r.decide(EngineError.Other("o"), PlaybackMethod.Remux, "h264") is Action.Fail)
    }

    @Test
    fun `media that ends short is a transcode for direct play, one new session for a stream, then a failure`() {
        val r = Recovery()
        assertEquals(Action.NewSession(PlaybackMode.Transcode, FallbackReason.Unplayable), r.onEndedShort(PlaybackMethod.Direct))
        assertEquals(Action.NewSession(null, null), r.onEndedShort(PlaybackMethod.Transcode))
        assertEquals(Action.Fail(Failure.Short, null), r.onEndedShort(PlaybackMethod.Transcode))
        // Retry gives the new session back, not the transcode: that was tried.
        r.retry()
        assertEquals(Action.NewSession(null, null), r.onEndedShort(PlaybackMethod.Direct))
        assertEquals(Action.Fail(Failure.Short, null), r.onEndedShort(PlaybackMethod.Direct))
    }

    @Test
    fun `five seconds of progress and Retry give the budgets back`() {
        val r = Recovery()
        repeat(3) { r.decide(EngineError.Playlist("p"), PlaybackMethod.Remux, null) }
        assertEquals(3, r.recreate)
        r.onPosition(10_000)
        r.onPosition(14_999)
        assertEquals(3, r.recreate)
        r.onPosition(15_000)
        assertEquals(0, r.recreate)
        repeat(3) { r.decide(EngineError.Playlist("p"), PlaybackMethod.Remux, null) }
        // A seek: progress counts from where it lands.
        r.onJump()
        r.onPosition(100_000)
        r.onPosition(104_000)
        assertEquals(3, r.recreate)
        r.retry()
        assertEquals(0, r.recreate)
    }

    @Test
    fun `stalls transcode a direct play with data, ask the server about streams, and count`() {
        val r = Recovery()
        assertEquals(Action.NewSession(PlaybackMode.Transcode, FallbackReason.NotSmooth), r.onStall(PlaybackMethod.Direct, starving = false))
        assertEquals(Action.CheckServer, r.onStall(PlaybackMethod.Transcode, starving = true))
        assertEquals(Action.SuggestLowerQuality, r.afterServerCheck(0.8, starving = true))
        assertEquals(Action.NewSession(null, null), r.afterServerCheck(2.0, starving = false))
        assertEquals(Action.Fail(Failure.Stuck, null), r.afterServerCheck(null, starving = false))
        repeat(3) { assertEquals(Action.NewSession(null, null), r.afterServerCheck(null, starving = true)) }
        assertEquals(Action.Fail(Failure.Stopped, null), r.afterServerCheck(null, starving = true))
        // A starving direct play gets new sessions too.
        val s = Recovery()
        assertEquals(Action.NewSession(null, null), s.onStall(PlaybackMethod.Direct, starving = true))
    }

    @Test
    fun `dropped frames switch once per item, Play original goes back, a new item starts over`() {
        val r = Recovery()
        assertNull(r.onStruggling(PlaybackMethod.Transcode))
        assertEquals(Action.NewSession(PlaybackMode.Transcode, FallbackReason.NotSmooth), r.onStruggling(PlaybackMethod.Direct))
        assertTrue(r.struggleSwitched)
        r.playOriginal()
        assertNull(r.fallbackMode)
        assertNull(r.onStruggling(PlaybackMethod.Direct))
        r.newItem()
        assertEquals(Action.NewSession(PlaybackMode.Transcode, FallbackReason.NotSmooth), r.onStruggling(PlaybackMethod.Remux))
    }
}
