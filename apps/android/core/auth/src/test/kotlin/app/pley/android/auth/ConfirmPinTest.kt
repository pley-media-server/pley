package app.pley.android.auth

import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.Dispatcher
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.RecordedRequest
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** The current profile's PIN, checked with the switch-profile grant (a TV at cold start). */
class ConfirmPinTest {
    private val server = MockWebServer()
    private val store = TokenStore(MemoryShelf(), testVault())
    private val issuer by lazy { issuerOf(server) }
    private val forms = mutableListOf<Map<String, String>>()
    private var answer: MockResponse = MockResponse().setBody(tokenAnswer("pley_at_same", "pley_rt_same"))
    private val kid = Profile(2, "Kid", null, null, current = true, needs = Needs.PIN)

    init {
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse = when (request.path) {
                "/api/oauth/token" -> {
                    synchronized(forms) { forms += formOf(request) }
                    answer
                }
                else -> MockResponse().setResponseCode(404)
            }
        }
    }

    @After
    fun stop() = server.shutdown()

    private fun profiles(): Pair<Profiles, Tokens> {
        val tokens = Tokens(issuer, store, endpointsClient())
        return Profiles(AuthHttp.api(endpointsClient(), tokens), tokens) to tokens
    }

    private fun refusal(error: String, description: String, reason: String? = null) = MockResponse().setResponseCode(400).setBody(
        JSONObject().put("error", error).put("error_description", description).apply { if (reason != null) put("reason", reason) }.toString(),
    )

    @Test
    fun theRightPinIsConfirmed() = runBlocking {
        store.save(savedTokens(issuer))
        val (profiles, _) = profiles()
        assertEquals(PinCheck.Confirmed, profiles.confirmPin(kid, "1234"))
        assertEquals(
            mapOf(
                "grant_type" to SWITCH_PROFILE_GRANT,
                "client_id" to CLIENT_ID,
                "refresh_token" to "pley_rt_one",
                "profile" to "2",
                "pin" to "1234",
            ),
            forms.single(),
        )
        // The answer's tokens are the ones kept.
        assertEquals("pley_rt_same", store.load(issuer)!!.refreshToken)
    }

    @Test
    fun serversThatCantCheckItGoOnWithout() = runBlocking {
        store.save(savedTokens(issuer))
        val (profiles, tokens) = profiles()
        answer = refusal("invalid_request", "the app uses this profile already")
        assertEquals(PinCheck.Unsupported, profiles.confirmPin(kid, "1234"))
        assertEquals("pley_rt_one", store.load(issuer)!!.refreshToken)
        assertTrue(tokens.signedIn.value)
    }

    @Test
    fun wrongPinsAreExplainedAndChangeNothing() = runBlocking {
        store.save(savedTokens(issuer))
        val (profiles, tokens) = profiles()
        for ((reason, message) in listOf(
            "wrong_pin" to "wrong PIN",
            // The server's own sentence says how long to wait, as for a switch.
            "throttled" to "too many attempts, try again in 12 min",
            "pin_locked" to "this profile's PIN is locked after too many wrong tries. Its owner or an administrator can reset it",
        )) {
            answer = refusal("invalid_grant", if (reason == "throttled") message else "no", reason)
            assertEquals(PinCheck.Failed(message), profiles.confirmPin(kid, "0000"))
            assertEquals("pley_rt_one", store.load(issuer)!!.refreshToken)
            assertTrue(tokens.signedIn.value)
        }
    }

    @Test
    fun aSpentSignInSignsOut() = runBlocking {
        store.save(savedTokens(issuer))
        val (profiles, _) = profiles()
        answer = refusal("invalid_grant", "spent")
        assertThrows(SignedOutException::class.java) { runBlocking { profiles.confirmPin(kid, "1234") } }
        Unit
    }
}
