package app.pley.android.player

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.UiText
import app.pley.android.common.resolve
import java.util.Locale

/**
 * [this] in English, as a screen shows it: for a test that reads the words
 * the player made (run on Robolectric, whose language is English).
 */
fun UiText.inEnglish(): String = resolve(ApplicationProvider.getApplicationContext<Context>().resources)

/** The app's context in the language of [tag] ("fr", "es-MX"), for a test that reads the words in it. */
fun inLanguage(tag: String): Context = ApplicationProvider.getApplicationContext<Context>()
    .createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })
