package app.pley.android.ui.tv.settings

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.UiText
import app.pley.android.common.errorSentence
import app.pley.android.common.resolve
import app.pley.android.ui.tv.R
import java.net.SocketTimeoutException
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The TV's settings, history and shell words that take a number or a
 * name: in English as they always were, and in French and Spanish, with
 * the arguments where each language puts them.
 */
@RunWith(RobolectricTestRunner::class)
class TvSettingsWordsTest {
    private val english = ApplicationProvider.getApplicationContext<Context>()

    private fun inLanguage(tag: String): Context =
        english.createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })

    private fun plays(count: Int, context: Context) = UiText.plural(R.plurals.tv_account_history_count, count).resolve(context)

    private fun preset(lines: String, mbps: Int, context: Context) = UiText.res(R.string.tv_settings_quality_preset, lines, mbps).resolve(context)

    private fun offline(context: Context) = UiText.res(R.string.tv_shell_offline_body, errorSentence(SocketTimeoutException())).resolve(context)

    @Test
    fun englishIsAsItWas() {
        assertEquals("1 play", plays(1, english))
        assertEquals("12 plays", plays(12, english))
        assertEquals("8 Mbps 1080p", preset("1080p", 8, english))
        assertEquals("You'll be signed out of nas:7539.", UiText.res(R.string.tv_account_sign_out_body, "nas:7539").resolve(english))
        assertEquals("Remove Pilot from history", UiText.res(R.string.tv_account_history_remove, "Pilot").resolve(english))
        assertEquals("No answer in time. Check that the server is on and that this device is on its network.", offline(english))
        assertEquals("On", UiText.res(R.string.tv_settings_switch_on).resolve(english))
        assertEquals("Off", UiText.res(R.string.tv_settings_switch_off).resolve(english))
    }

    @Test
    fun frenchAndSpanishSayTheSame() {
        val french = inLanguage("fr")
        val spanish = inLanguage("es")
        // French "one" covers 0 and 1.
        assertEquals("0 lecture", plays(0, french))
        assertEquals("1 lecture", plays(1, french))
        assertEquals("12 lectures", plays(12, french))
        assertEquals("1000000 de lectures", plays(1_000_000, french))
        assertEquals("1 reproducción", plays(1, spanish))
        assertEquals("0 reproducciones", plays(0, spanish))
        assertEquals("8 Mbit/s 1080p", preset("1080p", 8, french))
        assertEquals("30 Mbps 4K", preset("4K", 30, inLanguage("es-MX")))
        assertEquals("Cette TV sera déconnectée de nas:7539.", UiText.res(R.string.tv_account_sign_out_body, "nas:7539").resolve(inLanguage("fr-CA")))
        assertEquals("Quitar Pilot del historial", UiText.res(R.string.tv_account_history_remove, "Pilot").resolve(spanish))
        assertEquals(
            "Aucune réponse à temps. Vérifiez que le serveur est allumé et que cet appareil est sur son réseau.",
            offline(french),
        )
        assertEquals(
            "No hubo respuesta a tiempo. Comprueba que el servidor esté encendido y que este dispositivo esté en su red.",
            offline(spanish),
        )
        assertEquals("Activé", UiText.res(R.string.tv_settings_switch_on).resolve(french))
        assertEquals("Desactivado", UiText.res(R.string.tv_settings_switch_off).resolve(spanish))
    }

    @Test
    fun formatNamesAreTheSameInEveryLanguage() {
        assertEquals("Dolby Atmos", UiText.res(R.string.tv_settings_format_atmos).resolve(inLanguage("fr")))
        assertEquals("DTS-HD y DTS:X", UiText.res(R.string.tv_settings_format_dts_hd).resolve(inLanguage("es")))
    }
}
