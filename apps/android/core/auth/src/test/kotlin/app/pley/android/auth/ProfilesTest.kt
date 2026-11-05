package app.pley.android.auth

import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.Dispatcher
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.RecordedRequest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class ProfilesTest {
    private val server = MockWebServer()
    private val store = TokenStore(MemoryShelf(), testVault())
    private val issuer by lazy { issuerOf(server) }
    private val switches = mutableListOf<Map<String, String>>()
    private var switchAnswer: MockResponse = MockResponse().setBody(tokenAnswer("pley_at_kid", "pley_rt_kid"))

    init {
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse = when (request.path) {
                "/api/oauth/profiles" -> if (request.getHeader("Authorization") == "Bearer pley_at_one") {
                    MockResponse().setHeader("Content-Type", "application/json").setBody(
                        """[
                          {"id":1,"name":"Sam","avatar_url":"/api/users/1/avatar","avatar_color":"#e5a00d","current":true,"needs":"none"},
                          {"id":2,"name":"Kid","avatar_url":null,"avatar_color":null,"current":false,"needs":"pin"},
                          {"id":3,"name":"Admin","current":false,"needs":"password"},
                          {"id":4,"name":"Old","current":false,"needs":"locked"},
                          {"id":5,"name":"New","current":false,"needs":"retina_scan"}
                        ]""",
                    )
                } else {
                    MockResponse().setResponseCode(403).setHeader("WWW-Authenticate", "Bearer error=\"insufficient_scope\", scope=\"profiles\"")
                }
                "/api/oauth/token" -> {
                    synchronized(switches) { switches += formOf(request) }
                    switchAnswer
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

    private fun refusal(reason: String?, error: String = "invalid_grant", description: String = "no") = MockResponse().setResponseCode(400).setBody(
        org.json.JSONObject().put("error", error).put("error_description", description).apply { if (reason != null) put("reason", reason) }.toString(),
    )

    @Test
    fun theHouseholdIsListedWithWhatASwitchNeeds() = runBlocking {
        store.save(savedTokens(issuer))
        val (profiles, _) = profiles()
        assertTrue(profiles.available())
        val list = profiles.list()
        assertEquals(listOf(1L, 2L, 3L, 4L, 5L), list.map { it.id })
        assertEquals(listOf(Needs.NONE, Needs.PIN, Needs.PASSWORD, Needs.LOCKED, Needs.LOCKED), list.map { it.needs })
        assertEquals(Profile(1, "Sam", "/api/users/1/avatar", "#e5a00d", true, Needs.NONE), list[0])
        assertNull(list[1].avatarUrl)
    }

    @Test
    fun withoutTheProfilesScopeThereIsNoSwitching() = runBlocking {
        store.save(savedTokens(issuer, access = "pley_at_narrow", scope = "library play"))
        val (profiles, _) = profiles()
        assertFalse(profiles.available())
        assertThrows(AuthException::class.java) { runBlocking { profiles.list() } }
        Unit
    }

    @Test
    fun aSwitchUsesTheRefreshTokenAndThePin() = runBlocking {
        store.save(savedTokens(issuer))
        val (profiles, _) = profiles()
        val kid = Profile(2, "Kid", null, null, false, Needs.PIN)
        assertEquals(SwitchResult.NeedsPin, profiles.switchTo(kid))
        assertTrue(switches.isEmpty())
        assertEquals(SwitchResult.Switched, profiles.switchTo(kid, "1234"))
        assertEquals(
            mapOf(
                "grant_type" to "urn:pley:params:oauth:grant-type:switch-profile",
                "client_id" to CLIENT_ID,
                "refresh_token" to "pley_rt_one",
                "profile" to "2",
                "pin" to "1234",
            ),
            switches.single(),
        )
        assertEquals("pley_rt_kid", store.load(issuer)!!.refreshToken)
        assertEquals("pley_at_kid", store.load(issuer)!!.accessToken)

        // A profile that needs nothing gets no PIN.
        assertEquals(SwitchResult.Switched, profiles.switchTo(Profile(1, "Sam", null, null, false, Needs.NONE), "1234"))
        assertFalse(switches.last().containsKey("pin"))
    }

    @Test
    fun aWrongPinChangesNothing() = runBlocking {
        store.save(savedTokens(issuer))
        val (profiles, tokens) = profiles()
        val kid = Profile(2, "Kid", null, null, false, Needs.PIN)
        for ((reason, message) in listOf(
            "wrong_pin" to "wrong PIN",
            // The server's own sentence says how long to wait.
            "throttled" to "too many attempts, try again in 12 min",
            "pin_locked" to "this profile's PIN is locked after too many wrong tries. Its owner or an administrator can reset it",
            "not_allowed" to "you can't switch to this profile",
        )) {
            switchAnswer = refusal(reason, description = if (reason == "throttled") message else "no")
            assertEquals(SwitchResult.Failed(message), profiles.switchTo(kid, "0000"))
            assertEquals("pley_rt_one", store.load(issuer)!!.refreshToken)
            assertTrue(tokens.signedIn.value)
        }
    }

    @Test
    fun aPinSetSinceTheListWasReadIsAskedFor() = runBlocking {
        store.save(savedTokens(issuer))
        val (profiles, _) = profiles()
        // The list said none; the server wants a PIN now.
        switchAnswer = refusal("wrong_pin")
        assertEquals(SwitchResult.NeedsPin, profiles.switchTo(Profile(1, "Sam", null, null, false, Needs.NONE)))
        assertFalse(switches.single().containsKey("pin"))
        // A PIN that was sent is wrong.
        assertEquals(SwitchResult.Failed("wrong PIN"), profiles.switchTo(Profile(2, "Kid", null, null, false, Needs.PIN), "0000"))
    }

    @Test
    fun profilesThatNeedTheirPasswordSignInAgain() = runBlocking {
        store.save(savedTokens(issuer))
        val (profiles, tokens) = profiles()
        assertEquals(SwitchResult.SignInAgain, profiles.switchTo(Profile(3, "Admin", null, null, false, Needs.PASSWORD)))
        assertTrue(switches.isEmpty())
        // The server may know better than the list.
        switchAnswer = refusal("browser")
        assertEquals(SwitchResult.SignInAgain, profiles.switchTo(Profile(1, "Sam", null, null, false, Needs.NONE)))
        assertTrue(tokens.signedIn.value)
        assertEquals(SwitchResult.Failed("this profile can't be used from the app"), profiles.switchTo(Profile(4, "Old", null, null, false, Needs.LOCKED)))
    }

    @Test
    fun aCastsTicketSwitchesInPlaceOfAPinAndASpentOneChangesNothing() = runBlocking {
        store.save(savedTokens(issuer))
        val (_, tokens) = profiles()
        tokens.switchProfile(2, pin = null, ticket = "pley_st_one")
        assertEquals(
            mapOf(
                "grant_type" to "urn:pley:params:oauth:grant-type:switch-profile",
                "client_id" to CLIENT_ID,
                "refresh_token" to "pley_rt_one",
                "profile" to "2",
                "ticket" to "pley_st_one",
            ),
            switches.single(),
        )
        assertEquals("pley_rt_kid", store.load(issuer)!!.refreshToken)

        // Spent or expired: refused with its reason, and the sign-in stays as it was.
        switchAnswer = refusal("ticket_spent")
        val refused = assertThrows(TokenException::class.java) { runBlocking { tokens.switchProfile(2, pin = null, ticket = "pley_st_one") } }
        assertEquals("ticket_spent", refused.oauth?.reason)
        assertTrue(tokens.signedIn.value)
        assertEquals("pley_rt_kid", store.load(issuer)!!.refreshToken)
    }

    @Test
    fun otherRefusalsAreExplained() = runBlocking {
        store.save(savedTokens(issuer))
        val (profiles, tokens) = profiles()
        val sam = Profile(1, "Sam", null, null, false, Needs.NONE)
        switchAnswer = refusal(null, "unauthorized_client")
        assertEquals(SwitchResult.Failed("this sign-in can't switch profiles: sign in again to allow it"), profiles.switchTo(sam))
        // A spent refresh token: signed out.
        switchAnswer = refusal(null)
        assertThrows(SignedOutException::class.java) { runBlocking { profiles.switchTo(sam) } }
        assertFalse(tokens.signedIn.value)
        assertNull(store.load(issuer))
    }
}
