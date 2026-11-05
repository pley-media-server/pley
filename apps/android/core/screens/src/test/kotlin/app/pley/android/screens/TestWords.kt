package app.pley.android.screens

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.UiText
import app.pley.android.common.resolve

/**
 * [this] in English, as a screen shows it: for a test that reads the words
 * a ViewModel made (run on Robolectric, whose language is English).
 */
fun UiText?.inEnglish(): String? = this?.resolve(ApplicationProvider.getApplicationContext<Context>().resources)
