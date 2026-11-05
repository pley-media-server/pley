package app.pley.android.common

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import androidx.test.core.app.ApplicationProvider
import java.io.IOException
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Words made where nothing is drawn, put in the language of where they show. */
@RunWith(RobolectricTestRunner::class)
class UiTextTest {
    private val english = ApplicationProvider.getApplicationContext<Context>()

    private fun inLanguage(tag: String): Context =
        english.createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })

    @Test
    fun wordsTakeTheLanguageOfWhereTheyShow() {
        val text = UiText.res(R.string.error_no_answer)
        assertEquals("no answer in time", text.resolve(english))
        assertEquals("aucune réponse à temps", text.resolve(inLanguage("fr-CA")))
        assertEquals("no hubo respuesta a tiempo", text.resolve(inLanguage("es")))
    }

    @Test
    fun argumentsThatAreWordsAreResolvedFirst() {
        val nested = UiText.Sentence(UiText.res(R.string.error_nothing_answers))
        assertEquals("Nothing answers there.", nested.resolve(english))
        assertEquals("Rien ne répond à cette adresse.", nested.resolve(inLanguage("fr")))
        assertEquals("Português", UiText.raw("Português").resolve(inLanguage("fr")))
    }

    @Test
    fun anErrorSaysTheAppsWordsOrTheServersAsTheyAre() {
        val written = object : IOException("you were signed out: sign in again"), UserFacing {}
        assertEquals(UiText.raw("you were signed out: sign in again"), errorText(written))
        val own = object : IOException(), UserFacing {
            override val text: UiText = UiText.res(R.string.error_not_pley)
        }
        assertEquals("Ce n’est pas un serveur Pley.", errorSentence(own).resolve(inLanguage("fr")))
    }

    @Test
    fun theLanguagesAreNamedInThemselves() {
        for (language in Language.entries) {
            assertEquals(language.name, language.autonym.let(english::getString), language.autonym.let(inLanguage("es")::getString))
        }
        assertEquals("Français", english.getString(Language.French.autonym))
    }
}
