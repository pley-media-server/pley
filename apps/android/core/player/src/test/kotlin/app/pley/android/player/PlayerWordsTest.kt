package app.pley.android.player

import android.content.Context
import androidx.media3.common.PlaybackException
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.HttpDataSource
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.UiText
import app.pley.android.common.resolve
import app.pley.android.network.ApiException
import app.pley.android.player.live.LivePlayback
import app.pley.android.player.live.LiveTrouble
import app.pley.android.player.session.Qualities
import app.pley.android.player.session.ResourcePlayerTexts
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import app.pley.android.player.sleep.SleepTexts
import app.pley.android.player.subtitles.SubtitleSync
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** What the player says, in English as it always was, and in French and Spanish. */
@RunWith(RobolectricTestRunner::class)
@androidx.annotation.OptIn(UnstableApi::class)
class PlayerWordsTest {
    private val app = ApplicationProvider.getApplicationContext<Context>()

    private fun refused(status: Int) = PlaybackException(
        "refused",
        HttpDataSource.InvalidResponseCodeException(status, null, null, emptyMap(), DataSpec.Builder().setUri("http://x").build(), ByteArray(0)),
        PlaybackException.ERROR_CODE_IO_BAD_HTTP_STATUS,
    )

    @Test
    fun englishIsAsItWas() {
        val texts = ResourcePlayerTexts(app)
        assertEquals("No answer in time. Check that the server is on and that this device is on its network.", texts.unreachable(UiText.raw("no answer in time")))
        assertEquals("Couldn't find subtitles: the connection to the server failed", texts.searchFailed(UiText.raw("the connection to the server failed")))
        assertEquals("English · Made for this file · SDH · 1,234 downloads", texts.foundHint("English", exact = true, hearingImpaired = true, downloads = 1_234))
        assertEquals("1 download", texts.foundHint(null, exact = false, hearingImpaired = false, downloads = 1))
        assertEquals("2.5× real time · 40 s ready", texts.speed(2.5, 40))
        assertEquals("12.0× real time", texts.speed(12.0, 0))
        assertEquals(listOf("GPU", "GPU and CPU", "CPU", "copy", "vulkan"), listOf("gpu", "hybrid", "cpu", "copy", "vulkan").map(texts::pipeline))
        assertEquals("Video: video/hevc 3840×2160 23.976 fps", texts.engineVideo("video/hevc", 3840, 2160, 23.976f))
        assertEquals("Video: video/hevc 3840×2160", texts.engineVideo("video/hevc", 3840, 2160, null))
        assertEquals("Video decoder: c2.qti.hevc.decoder", texts.engineVideoDecoder("c2.qti.hevc.decoder"))
        assertEquals("Audio: audio/eac3 6 ch", texts.engineAudio("audio/eac3", 6))
        assertEquals("Audio decoder: c2.android.aac.decoder", texts.engineAudioDecoder("c2.android.aac.decoder"))
        assertEquals("Dropped frames: 3 of 5000", texts.engineDroppedFrames(3, 5_000))
        assertEquals("The server answered 503.", LiveTrouble.of(refused(503)).text.inEnglish())
        assertEquals("Live TV was stopped by the server's administrator.", LivePlayback.stoppedText(" ", byAdmin = true).inEnglish())
        assertEquals("1 min left", SleepTexts.summary(app.resources, SleepState(SleepChoice.Minutes(30), minutesLeft = 1)))
        assertEquals("42 min left", SleepTexts.summary(app.resources, SleepState(SleepChoice.Minutes(60), minutesLeft = 42)))
        assertEquals("1.25×", PlaybackSpeeds.label(1.25f))
        assertEquals("2×", PlaybackSpeeds.label(2f))
        assertEquals("−1.5 s", AudioDelays.label(-1_500))
        assertEquals("+0.5 s", SubtitleSync.label(500))
    }

    @Test
    fun frenchAndSpanishSayTheSame() {
        val french = inLanguage("fr").resources
        val spanish = inLanguage("es-MX").resources
        assertEquals("Le serveur a répondu avec le code 503.", LiveTrouble.of(refused(503)).text.resolve(french))
        assertEquals("El servidor respondió con el código 503.", LiveTrouble.of(refused(503)).text.resolve(spanish))
        assertEquals(
            "La TV en direct a été arrêtée par l’administrateur du serveur.\nMaintenance",
            LivePlayback.stoppedText("Maintenance", byAdmin = true).resolve(french),
        )
        // The server's own words stay as it wrote them, as a sentence.
        assertEquals("Bob is watching.", LiveTrouble.of(ApiException(410, "gone", "Bob is watching")).text.resolve(spanish))
        assertEquals("1 min restante", SleepTexts.summary(french, SleepState(SleepChoice.Minutes(30), minutesLeft = 1)))
        assertEquals("Queda 1 min", SleepTexts.summary(spanish, SleepState(SleepChoice.Minutes(30), minutesLeft = 1)))
        assertEquals("Quedan 42 min", SleepTexts.summary(spanish, SleepState(SleepChoice.Minutes(60), minutesLeft = 42)))
        assertEquals("Fin de ce titre", SleepTexts.choice(french, SleepChoice.EndOfItem, music = true))
        assertEquals("Mise en pause dans 30 s", SleepTexts.notice(french, 30))
        assertEquals("1 téléchargement", french.getQuantityString(R.plurals.session_search_downloads, 1, 1L))
        assertEquals("2 descargas", spanish.getQuantityString(R.plurals.session_search_downloads, 2, 2L))
        assertEquals("1,25×", PlaybackSpeeds.label(1.25f, Locale.FRENCH))
        assertEquals("+0,25 s", AudioDelays.label(250, Locale.forLanguageTag("es")))
        assertEquals("−1,2 s", SubtitleSync.label(-1_200, Locale.FRENCH))
        // A conversion is named the same in every language, as on the web.
        assertEquals("8 Mbps 1080p", french.getString(R.string.session_quality_8m))
        assertEquals("Automática · 8 Mbps 1080p", spanish.getString(R.string.session_quality_automatic_playing, spanish.getString(R.string.session_quality_8m)))
    }

    /** The session's words come from the app's resources in the language shown: here, French. */
    @Test
    @Config(qualifiers = "fr")
    fun theSessionSpeaksTheLanguageShown() {
        val texts = ResourcePlayerTexts(app)
        assertEquals(
            "Aucune réponse à temps. Vérifiez que le serveur est allumé et que cet appareil est sur son réseau.",
            texts.unreachable(UiText.raw("aucune réponse à temps")),
        )
        assertEquals("2,5× temps réel · 40 s d’avance", texts.speed(2.5, 40))
        assertEquals("Vidéo : video/hevc 3840×2160 23.976 i/s", texts.engineVideo("video/hevc", 3840, 2160, 23.976f))
        assertEquals("Lecture depuis le salon", texts.playingFrom(CastFrom("le salon")))
        assertEquals("Automatique · 8 Mbps 1080p", texts.automatic(texts.quality(Qualities.byId("8m"))))
        assertEquals("GPU et CPU", texts.pipeline("hybrid"))
    }
}
