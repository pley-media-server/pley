package app.pley.android.ui.phone.player

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import androidx.test.core.app.ApplicationProvider
import app.pley.android.designsystem.PleyColors
import app.pley.android.model.PlaybackMethod
import app.pley.android.ui.phone.R
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import app.pley.android.player.R as PlayerR

/** The player's words with numbers in them, and how it reads the session's, in English as they were and in French and Spanish. */
@RunWith(RobolectricTestRunner::class)
class PlayerWordsTest {
    private val english = ApplicationProvider.getApplicationContext<Context>()

    private fun inLanguage(tag: String): Context =
        english.createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })

    @Test
    fun theMethodBadgeTakesItsColourFromTheSessionsWordsInTheLanguageShown() {
        val r = english.resources
        assertEquals(PleyColors.Success, PlayerLook.badgeColors(PlayerLook.methodOf(r, "Direct play")).text)
        assertEquals(PleyColors.Info, PlayerLook.badgeColors(PlayerLook.methodOf(r, "Direct stream")).text)
        assertEquals(PleyColors.Info, PlayerLook.badgeColors(PlayerLook.methodOf(r, "Direct stream · Audio transcoding")).text)
        assertEquals(PleyColors.Accent, PlayerLook.badgeColors(PlayerLook.methodOf(r, "Transcoding")).text)
        // A download plays the file as it is, as the TV's badge says.
        assertEquals(PlaybackMethod.Direct, PlayerLook.methodOf(r, "Downloaded"))
        for (tag in listOf("fr", "es")) {
            val words = inLanguage(tag).resources
            assertEquals(tag, PlaybackMethod.Direct, PlayerLook.methodOf(words, words.getString(PlayerR.string.session_method_direct)))
            assertEquals(tag, PlaybackMethod.Remux, PlayerLook.methodOf(words, words.getString(PlayerR.string.session_method_remux_audio)))
            assertEquals(tag, PlaybackMethod.Transcode, PlayerLook.methodOf(words, words.getString(PlayerR.string.session_method_transcode)))
        }
    }

    @Test
    fun theDetailsWriteNumbersAsTheLanguageShownDoes() {
        assertEquals("23.976", frameRate(23.976f, Locale.ENGLISH))
        assertEquals("24", frameRate(24f, Locale.ENGLISH))
        assertEquals("29.970", frameRate(29.97f, Locale.ENGLISH))
        assertEquals("23,976", frameRate(23.976f, Locale.FRENCH))
        assertEquals("25", frameRate(25f, Locale.forLanguageTag("es")))

        assertEquals("23.976 fps", english.getString(R.string.player_details_frame_rate, "23.976"))
        assertEquals("23,976 i/s", inLanguage("fr").getString(R.string.player_details_frame_rate, "23,976"))
        assertEquals("23,976 fps", inLanguage("es").getString(R.string.player_details_frame_rate, "23,976"))

        assertEquals("1 ch", english.resources.getQuantityString(R.plurals.player_details_channels, 1, 1))
        assertEquals("6 ch", english.resources.getQuantityString(R.plurals.player_details_channels, 6, 6))
        assertEquals("1 canal", inLanguage("fr").resources.getQuantityString(R.plurals.player_details_channels, 1, 1))
        assertEquals("6 canaux", inLanguage("fr").resources.getQuantityString(R.plurals.player_details_channels, 6, 6))
        assertEquals("2 canales", inLanguage("es").resources.getQuantityString(R.plurals.player_details_channels, 2, 2))
    }

    @Test
    fun frenchAndSpanishSayTheScreensWords() {
        val french = inLanguage("fr")
        val spanish = inLanguage("es-MX")
        assertEquals("Play now (7)", english.getString(R.string.player_play_now_in, 7))
        assertEquals("Lire maintenant (7)", french.getString(R.string.player_play_now_in, 7))
        assertEquals("Reproducir ahora (7)", spanish.getString(R.string.player_play_now_in, 7))
        assertEquals("Mode de lecture : Transcodage", french.getString(R.string.player_method_hint, "Transcodage"))
        assertEquals("12 de 4512", spanish.getString(R.string.player_details_dropped_of, 12, 4512))
        assertEquals("Impossible de joindre votre serveur", french.getString(R.string.player_offline_title))
        assertEquals("Reproducción detenida", spanish.getString(R.string.player_stopped_title))
    }
}
