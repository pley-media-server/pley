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

/** The player's words with numbers in them, in English as they were and in French and Spanish, and its method badge's colours. */
@RunWith(RobolectricTestRunner::class)
class PlayerWordsTest {
    private val english = ApplicationProvider.getApplicationContext<Context>()

    private fun inLanguage(tag: String): Context =
        english.createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })

    @Test
    fun theMethodBadgeTakesItsColourFromTheWayItPlays() {
        // The way, not its words: the same in every language. A download plays as it is (direct).
        assertEquals(PleyColors.Success, PlayerLook.badgeColors(PlaybackMethod.Direct).text)
        assertEquals(PleyColors.Info, PlayerLook.badgeColors(PlaybackMethod.Remux).text)
        assertEquals(PleyColors.Accent, PlayerLook.badgeColors(PlaybackMethod.Transcode).text)
        assertEquals(PleyColors.Accent, PlayerLook.badgeColors(null).text)
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
