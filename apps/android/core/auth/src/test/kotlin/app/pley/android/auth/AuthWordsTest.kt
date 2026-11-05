package app.pley.android.auth

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.UiText
import app.pley.android.common.resolve
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** What signing in says, in English as it always was, and in French and Spanish. */
@RunWith(RobolectricTestRunner::class)
class AuthWordsTest {
    private val english = ApplicationProvider.getApplicationContext<Context>()

    private fun inLanguage(tag: String): Context =
        english.createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })

    @Test
    fun englishIsAsItWas() {
        assertEquals("this server is too old for this app", TOO_OLD.resolve(english))
        assertEquals(
            "this server's address isn't on your network and its connection isn't encrypted: open it by its https address to sign in",
            UNENCRYPTED.resolve(english),
        )
        assertEquals("this server can't sign in with a code yet: update it, or sign in with a password", NO_CODES.resolve(english))
        assertEquals("you were signed out: sign in again", SignOutReason.ENDED.text.resolve(english))
        assertEquals("the server allowed more than this app asks for: signed out", SignOutReason.WIDER_SCOPE.text.resolve(english))
        assertEquals(
            "this profile's PIN is locked after too many wrong tries. Its owner or an administrator can reset it",
            Profiles.PIN_LOCKED.resolve(english),
        )
        assertEquals(
            "this server calls itself http://nas:7539: open it by that address, or check the proxy in front of it passes Host",
            UiText.res(R.string.auth_other_issuer, "http://nas:7539").resolve(english),
        )
        assertEquals("the server couldn't answer (HTTP 502)", UiText.res(R.string.auth_server_failed, 502).resolve(english))
    }

    @Test
    fun frenchAndSpanishSayTheSame() {
        assertEquals("Ce serveur est trop ancien pour cette application.", UiText.Sentence(TOO_OLD).resolve(inLanguage("fr")))
        assertEquals("Este servidor es demasiado antiguo para esta aplicación.", UiText.Sentence(TOO_OLD).resolve(inLanguage("es-MX")))
        assertEquals("le serveur a refusé (HTTP 403)", UiText.res(R.string.auth_refused_status, 403).resolve(inLanguage("fr-CA")))
        assertEquals("PIN incorrecto", Profiles.message(OAuthError("invalid_grant", null, "wrong_pin")).resolve(inLanguage("es")))
        // The server's own words stay as it wrote them.
        assertEquals("too many tries", Profiles.message(OAuthError("invalid_grant", "too many tries", "throttled")).resolve(inLanguage("fr")))
    }

    @Test
    fun anErrorsMessageIsItsWords() {
        // Before the app is made (as here), the words as code; the app's own message is them in the language shown.
        val error = AuthException(TOO_OLD)
        assertEquals(TOO_OLD, error.text)
        assertEquals(TOO_OLD.toString(), error.message)
    }
}
