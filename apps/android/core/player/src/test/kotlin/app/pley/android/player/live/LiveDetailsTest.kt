package app.pley.android.player.live

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import app.pley.android.model.LivePlan
import app.pley.android.model.PlaybackMethod
import app.pley.android.player.PlaybackDetailsUi
import app.pley.android.player.inLanguage
import app.pley.android.testing.Fixtures
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** A channel's Playback details: the way it plays, why, and the rows, in the words of a video's. */
@RunWith(RobolectricTestRunner::class)
class LiveDetailsTest {
    private val english = ApplicationProvider.getApplicationContext<Context>().resources

    /** HEVC, 10-bit, 3840×2160 at 50, with E-AC3 5.1, at 12.3 Mbit/s: the server would make H.264 at 1080 lines and AAC of it. */
    private val hevc = Fixtures.decode<LivePlan>("live/plan_hevc.json")

    /** H.264 and AAC, sent as they are, as an older server says it: no bit depth, no bitrate. */
    private val h264 = Fixtures.decode<LivePlan>("live/plan.json")

    private fun rows(details: PlaybackDetailsUi?) = details?.rows?.map { it.label to it.value }

    @Test
    fun `a channel played as sent is Direct play, with what the provider sends`() {
        val details = LiveDetails.build(english, LivePlaying(hevc, asSent = true))
        assertEquals("Direct play", details?.method)
        assertEquals(PlaybackMethod.Direct, details?.playbackMethod)
        assertNull(details?.why)
        // What the server would make of it doesn't reach this player: no arrows, no encoder.
        assertEquals(listOf("Video" to "HEVC 10-bit · 3840×2160 · 50 fps", "Audio" to "EAC3 5.1", "Bitrate" to "12 Mbps"), rows(details))
    }

    @Test
    fun `a channel the server converts is Transcoding, with what it is made into and why`() {
        val details = LiveDetails.build(english, LivePlaying(hevc, asSent = false, why = NotAsSent.Video))
        assertEquals("Transcoding", details?.method)
        assertEquals(PlaybackMethod.Transcode, details?.playbackMethod)
        assertEquals("This device doesn't play HEVC", details?.why)
        assertEquals(
            listOf(
                "Video" to "HEVC 10-bit · 3840×2160 · 50 fps → H.264 1080p",
                "Audio" to "EAC3 5.1 → AAC 2.0",
                "Bitrate" to "12 Mbps → 8.0 Mbps",
                "Encoder" to "NVIDIA NVENC",
            ),
            rows(details),
        )
        // Interlaced, and no height said for the transcode: the provider's lines. An encoder with no name of its own stays as it is.
        val sd = hevc.copy(videoCodec = "mpeg2video", width = 720, height = 576, frameRate = 25.0, interlaced = true, bitDepth = null, outHeight = null, videoKbps = null, kbps = 850, video = "h264_rkmpp")
        assertEquals(
            listOf("Video" to "MPEG-2 · 720×576 · 25 fps · interlaced → H.264 576p", "Audio" to "EAC3 5.1 → AAC 2.0", "Bitrate" to "850 kbps", "Encoder" to "h264_rkmpp"),
            rows(LiveDetails.build(english, LivePlaying(sd, asSent = false, why = NotAsSent.Interlaced))),
        )
    }

    @Test
    fun `HLS with its video copied is Direct stream, and says when the server converts its sound, as a video's`() {
        val plan = h264.copy(audio = "aac", audioCodec = "ac3", audioChannels = 6)
        val details = LiveDetails.build(english, LivePlaying(plan, asSent = false, why = NotAsSent.Audio))
        assertEquals("Direct stream · Audio transcoding", details?.method)
        assertEquals("Flux direct · Audio transcodé", LiveDetails.build(inLanguage("fr").resources, LivePlaying(plan, asSent = false))?.method)
        assertEquals(PlaybackMethod.Remux, details?.playbackMethod)
        assertEquals("This device doesn't play AC3 sound", details?.why)
        // An older server: no bit depth and no bitrate to show, and nothing encodes the video.
        assertEquals(listOf("Video" to "H.264 · 1280×720 · 25 fps", "Audio" to "AC3 5.1 → AAC 2.0"), rows(details))
        // Copied whole and on HLS all the same (its stream.ts failed): nothing converted, no reason.
        val copied = LiveDetails.build(english, LivePlaying(h264, asSent = false))
        assertEquals("Direct stream", copied?.method)
        assertNull(copied?.why)
        assertEquals(listOf("Video" to "H.264 · 1280×720 · 25 fps", "Audio" to "AAC 2.0"), rows(copied))
    }

    @Test
    fun `why says each reason in a sentence, and nothing when there is none to tell`() {
        fun why(reason: NotAsSent?) = LiveDetails.build(english, LivePlaying(hevc, asSent = false, why = reason))?.why
        assertEquals("Interlaced video is made progressive", why(NotAsSent.Interlaced))
        assertEquals("This device doesn't play HEVC", why(NotAsSent.Video))
        assertEquals("This device doesn't play EAC3 sound", why(NotAsSent.Audio))
        assertNull(why(NotAsSent.Unknown))
        assertNull(why(null))
    }

    @Test
    fun `rows whose value isn't known are left out`() {
        // A radio channel: no picture.
        val radio = LivePlan(audioCodec = "mp2", audioChannels = 1, video = "none", audio = "aac")
        assertEquals(listOf("Audio" to "MP2 1.0 → AAC 2.0"), rows(LiveDetails.build(english, LivePlaying(radio, asSent = false, why = NotAsSent.Audio))))
        assertEquals(listOf("Audio" to "MP2 1.0"), rows(LiveDetails.build(english, LivePlaying(radio, asSent = true))))
        // Nothing probed of it but that it is copied.
        assertEquals(emptyList<Pair<String, String>>(), rows(LiveDetails.build(english, LivePlaying(LivePlan(video = "copy", audio = "none"), asSent = true))))
        // No plan: as sent, the way alone; on HLS, nothing to say of it.
        val alone = LiveDetails.build(english, LivePlaying(null, asSent = true))
        assertEquals("Direct play", alone?.method)
        assertEquals(emptyList<Pair<String, String>>(), rows(alone))
        assertNull(LiveDetails.build(english, LivePlaying(null, asSent = false)))
    }

    @Test
    fun `the details are in the language shown`() {
        val french = LiveDetails.build(inLanguage("fr").resources, LivePlaying(hevc.copy(interlaced = true), asSent = false, why = NotAsSent.Video))
        assertEquals("Transcodage", french?.method)
        assertEquals("Cet appareil ne lit pas la vidéo HEVC", french?.why)
        assertEquals(
            listOf(
                "Vidéo" to "HEVC 10 bits · 3840×2160 · 50 i/s · entrelacé → H.264 1080p",
                "Audio" to "EAC3 5.1 → AAC 2.0",
                "Débit" to "12 Mbit/s → 8,0 Mbit/s",
                "Encodeur" to "NVIDIA NVENC",
            ),
            rows(french),
        )
        fun why(tag: String, reason: NotAsSent) = LiveDetails.build(inLanguage(tag).resources, LivePlaying(hevc, asSent = false, why = reason))?.why
        assertEquals("La vidéo entrelacée est convertie en progressif", why("fr", NotAsSent.Interlaced))
        assertEquals("Cet appareil ne lit pas l’audio EAC3", why("fr", NotAsSent.Audio))
        assertEquals("El video entrelazado se convierte a progresivo", why("es-MX", NotAsSent.Interlaced))
        assertEquals("Este dispositivo no reproduce HEVC", why("es-MX", NotAsSent.Video))
        assertEquals("Este dispositivo no reproduce el audio EAC3", why("es-MX", NotAsSent.Audio))
        assertEquals("隔行扫描视频会转换为逐行扫描", why("zh-CN", NotAsSent.Interlaced))
        assertEquals("此设备无法播放 EAC3 音频", why("zh-CN", NotAsSent.Audio))
        assertEquals("隔行掃描影片會轉換為逐行掃描", why("zh-TW", NotAsSent.Interlaced))
        assertEquals("此裝置無法播放 HEVC", why("zh-TW", NotAsSent.Video))
    }
}
