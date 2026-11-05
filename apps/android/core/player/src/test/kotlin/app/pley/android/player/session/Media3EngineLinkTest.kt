package app.pley.android.player.session

import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.test.utils.FakeMediaSource
import androidx.media3.test.utils.FakeTimeline
import androidx.media3.test.utils.TestExoPlayerBuilder
import androidx.media3.test.utils.robolectric.TestPlayerRunHelper
import app.pley.android.player.EngineSource
import app.pley.android.player.PlayerEngine
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment

/** The link over a real ExoPlayer with fake renderers and media: its events, and tracks chosen by their order. */
@RunWith(RobolectricTestRunner::class)
@androidx.annotation.OptIn(UnstableApi::class)
class Media3EngineLinkTest {
    private val exo: ExoPlayer = TestExoPlayerBuilder(RuntimeEnvironment.getApplication()).build()
    private val formats = arrayOf(
        Format.Builder().setSampleMimeType(MimeTypes.VIDEO_H264).setWidth(1920).setHeight(1080).build(),
        Format.Builder().setSampleMimeType(MimeTypes.AUDIO_AAC).setLanguage("en").setChannelCount(2).setSampleRate(48_000).build(),
        Format.Builder().setSampleMimeType(MimeTypes.AUDIO_AAC).setLanguage("fr").setChannelCount(2).setSampleRate(48_000).build(),
        Format.Builder().setSampleMimeType(MimeTypes.TEXT_VTT).setLanguage("en").setSelectionFlags(C.SELECTION_FLAG_DEFAULT).build(),
        Format.Builder().setSampleMimeType(MimeTypes.TEXT_VTT).setLanguage("fr").build(),
    )
    private val prepared = mutableListOf<Pair<EngineSource, Long>>()
    private var released = false
    private val engine = object : PlayerEngine {
        override val player: Player = exo

        override fun prepare(source: EngineSource, startMs: Long, playWhenReady: Boolean) {
            prepared += source to startMs
            exo.setMediaSource(FakeMediaSource(FakeTimeline(), *formats))
            exo.playWhenReady = playWhenReady
            exo.prepare()
        }

        override fun release() {
            released = true
            exo.release()
        }
    }
    private val events = mutableListOf<String>()
    private val link = Media3EngineLink(engine, ResourcePlayerTexts(RuntimeEnvironment.getApplication())).apply {
        setListener(
            object : EngineLink.Listener {
                override fun onStatus(status: EngineStatus) {
                    events += "status $status"
                }

                override fun onTracks(tracks: EngineTracks) {
                    events += "tracks ${tracks.video.size}/${tracks.audio.size}/${tracks.text.size}"
                }

                override fun onPlayWhenReady(playWhenReady: Boolean) {
                    events += "playWhenReady $playWhenReady"
                }
            },
        )
    }

    @After
    fun release() {
        if (!released) link.release()
    }

    private fun overrideLanguage(type: Int): String? =
        exo.trackSelectionParameters.overrides.values.firstOrNull { it.type == type }?.mediaTrackGroup?.getFormat(0)?.language

    @Test
    fun `it reports the engine's states and the media's tracks`() {
        link.prepare(EngineSource("http://pley/api/files/1/stream", hls = false), 0, playWhenReady = false)
        TestPlayerRunHelper.advance(exo).untilState(Player.STATE_READY)
        assertEquals(EngineStatus.Ready, link.status)
        assertTrue("status Buffering" in events)
        assertTrue("status Ready" in events)
        assertTrue("tracks 1/2/2" in events)
        link.play()
        assertTrue(link.playWhenReady)
        assertTrue("playWhenReady true" in events)
        link.release()
        assertTrue(released)
    }

    @Test
    fun `tracks are chosen by their order in the file, checked on the language, and subtitles stay off`() {
        link.prepare(EngineSource("x", hls = false), 0, playWhenReady = false)
        // Before the tracks are known: applied when they are.
        link.selectAudio(1, "fre")
        TestPlayerRunHelper.advance(exo).untilState(Player.STATE_READY)
        assertEquals("fr", overrideLanguage(C.TRACK_TYPE_AUDIO))
        // The file flags English subtitles as default: off is off.
        assertTrue(C.TRACK_TYPE_TEXT in exo.trackSelectionParameters.disabledTrackTypes)

        link.selectText(1, "fre")
        assertFalse(C.TRACK_TYPE_TEXT in exo.trackSelectionParameters.disabledTrackTypes)
        assertEquals("fr", overrideLanguage(C.TRACK_TYPE_TEXT))

        // The order says English, the language French: the language wins.
        link.selectAudio(0, "fre")
        assertEquals("fr", overrideLanguage(C.TRACK_TYPE_AUDIO))
        link.selectAudio(0, "eng")
        assertEquals("en", overrideLanguage(C.TRACK_TYPE_AUDIO))

        link.selectText(null, null)
        assertTrue(C.TRACK_TYPE_TEXT in exo.trackSelectionParameters.disabledTrackTypes)

        // A new media forgets the choices.
        link.prepare(EngineSource("y", hls = true), 5_000, playWhenReady = false)
        assertEquals(null, overrideLanguage(C.TRACK_TYPE_AUDIO))
        assertEquals(5_000L, prepared.last().second)
    }
}
