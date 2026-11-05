package app.pley.android.ui.phone.shell

import android.content.Context
import android.content.res.Configuration
import android.content.res.Resources
import android.os.LocaleList
import androidx.test.core.app.ApplicationProvider
import app.pley.android.ui.phone.R
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The words of Me, Settings, the remote, downloads and the kit that take numbers and names, in English, French and Spanish. */
@RunWith(RobolectricTestRunner::class)
class ShellWordsTest {
    private val english = ApplicationProvider.getApplicationContext<Context>().resources

    private fun inLanguage(tag: String): Resources =
        ApplicationProvider.getApplicationContext<Context>()
            .createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })
            .resources

    @Test
    fun englishIsAsItWas() {
        assertEquals("Remote, 1 device playing", english.getQuantityString(R.plurals.remote_button_playing, 1, 1))
        assertEquals("Remote, 3 devices playing", english.getQuantityString(R.plurals.remote_button_playing, 3, 3))
        assertEquals("3 downloads · 4.2 GB", english.getQuantityString(R.plurals.downloads_me_count, 3, 3, "4.2 GB"))
        assertEquals("Downloading · 45% of 1.4 GB", english.getString(R.string.downloads_progress, 45, "1.4 GB"))
        assertEquals("3.5 stars", english.getQuantityString(R.plurals.kit_rating_stars, 2, "3.5"))
        assertEquals("8 Mbps 1080p", english.getString(R.string.settings_quality_preset, "1080p", 8))
    }

    @Test
    fun frenchAndSpanishSayTheSame() {
        val french = inLanguage("fr")
        val spanish = inLanguage("es")
        // French one covers 0 and 1.
        assertEquals("Télécommande, 1 appareil en lecture", french.getQuantityString(R.plurals.remote_button_playing, 1, 1))
        assertEquals("Télécommande, 0 appareil en lecture", french.getQuantityString(R.plurals.remote_button_playing, 0, 0))
        assertEquals("Control remoto, 3 dispositivos reproduciendo", spanish.getQuantityString(R.plurals.remote_button_playing, 3, 3))
        assertEquals("1 épisode · 2,1 Go", french.getQuantityString(R.plurals.downloads_episodes, 1, 1, "2,1 Go"))
        assertEquals("1000000 de téléchargements · 9 To", french.getQuantityString(R.plurals.downloads_me_count, 1_000_000, 1_000_000, "9 To"))
        assertEquals("Descargando · 45\u00a0% de 1,4 GB", spanish.getString(R.string.downloads_progress, 45, "1,4 GB"))
        assertEquals("8\u00a0Mbit/s 1080p", french.getString(R.string.settings_quality_preset, "1080p", 8))
        assertEquals("¿Detener la reproducción en Salon?", spanish.getString(R.string.remote_stop_confirm, "Salon"))
        assertEquals("Arrêter la lecture sur Salon\u00a0?", french.getString(R.string.remote_stop_confirm, "Salon"))
    }
}
