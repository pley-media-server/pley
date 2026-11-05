package app.pley.android.downloads

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.resolve
import app.pley.android.data.DownloadState
import app.pley.android.model.ItemKind
import app.pley.android.network.ApiException
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Why a download failed, in English as it always was, and in French and Spanish. */
@RunWith(RobolectricTestRunner::class)
class DownloadWordsTest {
    private val english = ApplicationProvider.getApplicationContext<Context>()

    private fun inLanguage(tag: String): Context =
        english.createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })

    private val saved = Saved(server = "http://nas:7539", userId = 1, itemId = 42, fileId = 7, kind = ItemKind.Movie, title = "Movie", addedAt = 0)

    private fun failed(failure: Failure, error: String? = null, status: Int? = null) =
        saved.copy(state = DownloadState.Failed, failure = failure, error = error, status = status)

    @Test
    fun englishIsAsItWas() {
        assertEquals("The downloaded file is gone", failed(Failure.Missing).errorText()?.resolve(english))
        assertEquals("Not enough space on this device", failed(Failure.NoSpace).errorText()?.resolve(english))
        assertEquals(
            "The server can't make smaller downloads: choose Original in Settings → Downloads",
            failed(Failure.NoConversions).errorText()?.resolve(english),
        )
        assertEquals("The server couldn't make it for this device", failed(Failure.ConversionFailed).errorText()?.resolve(english))
        assertEquals(
            "The server couldn't make it for this device: ffmpeg exited",
            failed(Failure.ConversionFailed, error = "ffmpeg exited").errorText()?.resolve(english),
        )
        assertEquals("Sign in to the server again to download this", saved.failedWith(ApiException(401, "unauthorized", "no")).errorText()?.resolve(english))
        assertEquals("This file is no longer on the server", saved.failedWith(ApiException(410, "gone", "gone")).errorText()?.resolve(english))
        assertEquals("The server couldn't send this file (502)", saved.failedWith(ApiException(502, "bad_gateway", "down")).errorText()?.resolve(english))
    }

    @Test
    fun whatAnOlderVersionKeptReadsAsItWasWritten() {
        assertEquals("Not enough space on this device", saved.copy(error = "Not enough space on this device").errorText()?.resolve(inLanguage("fr")))
        assertNull(saved.errorText())
    }

    @Test
    fun frenchAndSpanishSayTheSame() {
        assertEquals("Ce fichier n’est plus sur le serveur", failed(Failure.Gone).errorText()?.resolve(inLanguage("fr")))
        assertEquals("El servidor no pudo enviar este archivo (502)", failed(Failure.ServerFailed, status = 502).errorText()?.resolve(inLanguage("es-MX")))
        // The server's own words stay as it wrote them.
        assertEquals(
            "Le serveur n’a pas pu le préparer pour cet appareil\u00a0: ffmpeg exited",
            failed(Failure.ConversionFailed, error = "ffmpeg exited").errorText()?.resolve(inLanguage("fr-CA")),
        )
    }
}
