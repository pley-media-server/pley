package app.pley.android.network

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import androidx.test.core.app.ApplicationProvider
import app.pley.android.auth.R as AuthR
import app.pley.android.common.UiText
import app.pley.android.common.errorSentence
import app.pley.android.common.errorText
import app.pley.android.common.resolve
import java.io.IOException
import java.util.Locale
import kotlinx.serialization.SerializationException
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** What the network's errors say, in English as it always was, and in French and Spanish. */
@RunWith(RobolectricTestRunner::class)
class NetworkWordsTest {
    private val english = ApplicationProvider.getApplicationContext<Context>()

    private fun inLanguage(tag: String): Context =
        english.createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })

    private val blocked = OfflineException(IOException("connect failed: EPERM (Operation not permitted)"))
    private val unreadable = UnreadableAnswerException(200, SerializationException("bad"))

    @Test
    fun englishIsAsItWas() {
        assertEquals("Pley isn't allowed on your home network: allow it in the app's permissions", errorText(blocked).resolve(english))
        assertEquals("the server's answer can't be read: is it Pley, and up to date?", errorText(unreadable).resolve(english))
        assertEquals("No answer in time.", errorSentence(OfflineException(java.net.SocketTimeoutException("timeout"))).resolve(english))
        // ApiException.from's words without the server's.
        assertEquals("the server refused (HTTP 500)", UiText.res(AuthR.string.auth_refused_status, 500).resolve(english))
    }

    @Test
    fun frenchAndSpanishSayTheSame() {
        assertEquals(
            "Pley n’a pas accès au réseau de votre domicile : autorisez-le dans les autorisations de l’application.",
            errorSentence(blocked).resolve(inLanguage("fr")),
        )
        assertEquals("No se puede leer la respuesta del servidor: ¿es Pley y está actualizado?", errorSentence(unreadable).resolve(inLanguage("es-MX")))
        assertEquals("Aucune réponse à temps.", errorSentence(OfflineException(java.net.SocketTimeoutException("timeout"))).resolve(inLanguage("fr-CA")))
    }

    @Test
    fun aRefusalOfTheAppsOwnWordsIsThem() {
        val words = UiText.res(AuthR.string.auth_refused_status, 503)
        val refusal = ApiException(503, "unavailable", words)
        assertEquals(words, refusal.text)
        assertEquals(words, errorText(refusal))
        // Before the app is made (as here), the words as code; in the app, them in the language shown.
        assertEquals(words.toString(), refusal.message)
    }
}
