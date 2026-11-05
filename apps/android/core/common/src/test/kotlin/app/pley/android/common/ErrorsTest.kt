package app.pley.android.common

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import java.io.EOFException
import java.io.IOException
import java.net.ConnectException
import java.net.SocketTimeoutException
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** What people read of a failure: the app's words, never OkHttp's; in English, as it always said. */
@RunWith(RobolectricTestRunner::class)
class ErrorsTest {
    private val english = ApplicationProvider.getApplicationContext<Context>()

    private fun said(error: Throwable): String = errorText(error).resolve(english)

    @Test
    fun aKnownFailureHasItsOwnWords() {
        assertEquals("nothing answers there", said(ConnectException("Failed to connect to /192.168.1.20:7539")))
        assertEquals("no answer in time", said(SocketTimeoutException("timeout")))
        assertEquals("No answer in time.", errorSentence(SocketTimeoutException("timeout")).resolve(english))
    }

    @Test
    fun anyOtherNetworkFailureIsOneSentence() {
        assertEquals("the connection to the server failed", said(IOException("unexpected end of stream on http://192.168.1.20:7539/...")))
        assertEquals("the connection to the server failed", said(EOFException("\\n not found: limit=0 content=…")))
        assertEquals("The connection to the server failed.", errorSentence(IOException("Canceled")).resolve(english))
    }

    @Test
    fun wordsWrittenForPeopleStayAsTheyAre() {
        val written = object : IOException("you were signed out: sign in again"), UserFacing {}
        assertEquals("you were signed out: sign in again", said(written))
        assertEquals("downloads isn't built yet", said(NotBuiltYet("downloads")))
        assertEquals("not signed in", said(IllegalStateException("not signed in")))
    }
}
