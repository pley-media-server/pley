package app.pley.android.network

import app.pley.android.common.UiText
import app.pley.android.common.errorText
import java.io.IOException
import java.net.ConnectException
import kotlinx.serialization.SerializationException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class ApiExceptionTest {
    @Test
    fun bearerChallengesAreRead() {
        assertEquals(BearerChallenge("invalid_token", null), BearerChallenge.of(listOf("""Bearer error="invalid_token"""")))
        assertEquals(
            BearerChallenge("insufficient_scope", "play"),
            BearerChallenge.of(listOf("""Bearer error="insufficient_scope", scope="play"""")),
        )
        assertEquals(BearerChallenge("invalid_request", null), BearerChallenge.of(listOf("bearer error=invalid_request")))
        // Another scheme first, then pley's.
        assertEquals(
            BearerChallenge("invalid_token", null),
            BearerChallenge.of(listOf("""Basic realm="x"""", """Bearer realm="pley", error="invalid_token"""")),
        )
        assertNull(BearerChallenge.of(listOf("""Basic realm="x"""")))
        assertNull(BearerChallenge.of(emptyList()))
        assertNull(BearerChallenge.of(listOf("Bearerish error=x")))
    }

    @Test
    fun refusalsSayWhatTheyMean() {
        assertTrue(ApiException(401, "unauthorized", "x", bearerError = "invalid_token").signedOut)
        assertTrue(ApiException(403, "insufficient_scope", "x").needsScope)
        assertTrue(ApiException(403, null, "x", bearerError = "insufficient_scope").needsScope)
        assertTrue(!ApiException(403, "forbidden", "x").needsScope)
    }

    @Test
    fun failuresAreSortedOut() {
        val refusal = ApiException(404, "not_found", "x")
        assertSame(refusal, failure(refusal))
        assertTrue(failure(ConnectException("refused")) is OfflineException)
        assertTrue(failure(SerializationException("bad")) is UnreadableAnswerException)
        val offline = failure(IOException("connect failed: EPERM (Operation not permitted)")) as OfflineException
        assertTrue(offline.blocked)
        // "Pley isn't allowed on your home network: allow it in the app's permissions" (NetworkWordsTest).
        assertEquals(UiText.res(R.string.network_not_allowed), offline.text)
        // Any other failure is worded as errorText words its cause.
        assertEquals(errorText(ConnectException("refused")), (failure(ConnectException("refused")) as OfflineException).text)
        val signedOut = app.pley.android.auth.SignedOutException()
        assertSame(signedOut, failure(signedOut))
    }
}
