package app.pley.android.screens.account

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.formatWords
import app.pley.android.model.format.FormatWords
import java.text.Collator
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The audio and subtitle languages named and sorted in the language shown. */
@RunWith(RobolectricTestRunner::class)
class TrackLanguagesTest {
    private fun wordsIn(tag: String): FormatWords = formatWords(
        ApplicationProvider.getApplicationContext<Context>()
            .createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })
            .resources,
    )

    @Test
    fun englishIsAsItWas() {
        val all = TrackLanguages.all(wordsIn("en"))
        assertEquals(TrackLanguages.all(FormatWords.English), all)
        assertEquals("German", TrackLanguages.nameOf("ger", wordsIn("en")))
    }

    @Test
    fun frenchAndSpanishNameThemInTheirOwnOrder() {
        val french = TrackLanguages.all(wordsIn("fr"))
        assertEquals(34, french.size)
        assertEquals("Allemand", TrackLanguages.nameOf("ger", wordsIn("fr")))
        assertEquals("Anglais", french.single { it.first == "eng" }.second)
        val frenchOrder = Collator.getInstance(Locale.FRENCH)
        assertEquals(french.map { it.second }.sortedWith(frenchOrder), french.map { it.second })

        val spanish = TrackLanguages.all(wordsIn("es"))
        assertEquals("Inglés", TrackLanguages.nameOf("eng", wordsIn("es")))
        // "Árabe" goes with the A's, not after "Vietnamita" as by code points.
        assertEquals(listOf("Alemán", "Árabe", "Búlgaro"), spanish.take(3).map { it.second })
    }
}
