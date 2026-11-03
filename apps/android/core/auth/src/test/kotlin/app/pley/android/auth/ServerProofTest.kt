package app.pley.android.auth

import app.pley.android.common.describe
import java.util.Base64
import kotlinx.coroutines.runBlocking
import okhttp3.Request
import okhttp3.mockwebserver.Dispatcher
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.RecordedRequest
import okhttp3.mockwebserver.SocketPolicy
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** The server proves it is the one signed in to before a token goes to its address (docs/API.md, "Telling servers apart"). */
class ServerProofTest {
    private val server = MockWebServer()
    private val shelf = MemoryShelf()
    private val store = TokenStore(shelf, testVault())
    private var now = 10_000_000L
    private val issuer by lazy { issuerOf(server) }

    /** The address the proofs cover: the issuer, as the server reads it. */
    private val address by lazy { app.pley.android.common.Addresses.origin(issuer)!! }

    private val key = ByteArray(32) { 1 }
    private val keyText = Base64.getUrlEncoder().withoutPadding().encodeToString(key)
    private val proof = ServerProof(SERVER_ID, "7", keyText)

    /** The challenges `/api/server` was asked, and what it answers to each. */
    private val challenges = mutableListOf<String>()
    private var answer: (challenge: String) -> MockResponse = { challenge ->
        MockResponse().setBody(JSONObject().put("id", SERVER_ID).put("proof", ServerProofs.proof(key, SERVER_ID, address, challenge)).toString())
    }
    private val revokes = mutableListOf<String>()
    private var refreshes = 0
    private var refreshAnswer: () -> String = { tokenAnswer("pley_at_new${++refreshes}", "pley_rt_new$refreshes") }

    init {
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse {
                val url = request.requestUrl!!
                return when (url.encodedPath) {
                    "/api/server" -> {
                        val challenge = url.queryParameter("challenge")!!
                        assertEquals(proof.proofId, url.queryParameter("proof_id"))
                        synchronized(challenges) { challenges += challenge }
                        answer(challenge)
                    }
                    "/api/oauth/token" -> MockResponse().setBody(refreshAnswer())
                    "/api/oauth/revoke" -> {
                        synchronized(revokes) { revokes += formOf(request)["token"]!! }
                        MockResponse()
                    }
                    else -> MockResponse().setBody("{}")
                }
            }
        }
    }

    @After
    fun stop() = server.shutdown()

    private fun tokens() = Tokens(issuer, store, endpointsClient()) { now }

    private fun signedIn(expiresAt: Long = Long.MAX_VALUE / 2) = store.save(savedTokens(issuer, expiresAt = expiresAt).copy(proof = proof))

    @Test
    fun theProofIsTheServersKnownAnswer() {
        // The same as the server's and the TVs' tests.
        assertEquals(
            "YwkvaCQfaIQCZkoOP5yOE3yR4xRKU5T5wYWz4b85SCM",
            ServerProofs.proof(key, "0123456789abcdef0123456789abcdef01234567", "http://192.168.1.20:7539", "AAECAwQFBgcICQoLDA0ODw"),
        )
    }

    @Test
    fun tokenAnswersCarryTheProofWhenWrittenAsTheyShould() {
        fun parse(extra: String) = TokenResponse.parse("""{"access_token":"a","token_type":"Bearer","expires_in":60$extra}""")
        assertEquals(proof, parse(""","server_id":"$SERVER_ID","proof_id":"7","proof_key":"$keyText"""").proof)
        // An older server: nothing to check.
        assertNull(parse("").proof)
        for (bad in listOf(
            ""","server_id":"$SERVER_ID","proof_id":"07","proof_key":"$keyText"""",
            ""","server_id":"$SERVER_ID","proof_id":"0","proof_key":"$keyText"""",
            ""","server_id":"$SERVER_ID","proof_id":"12345678901234567890","proof_key":"$keyText"""",
            ""","server_id":"$SERVER_ID","proof_id":7,"proof_key":"$keyText"""",
            ""","server_id":"$SERVER_ID","proof_id":"7","proof_key":"$keyText="""",
            ""","server_id":"$SERVER_ID","proof_id":"7","proof_key":"${keyText.dropLast(2)}"""",
            ""","server_id":"$SERVER_ID","proof_id":"7","proof_key":"${keyText.replace('-', '+')}A"""",
            ""","server_id":"","proof_id":"7","proof_key":"$keyText"""",
            ""","proof_id":"7","proof_key":"$keyText"""",
        )) {
            assertNull(bad, parse(bad).proof)
        }
        // Kept with the tokens, never printed.
        val kept = savedTokens(issuer).copy(proof = proof)
        assertEquals(kept, StoredTokens.fromJson(kept.toJson()))
        assertFalse(proof.toString().contains(keyText))
    }

    @Test
    fun aRefreshWithoutAProofKeepsTheOneBefore() {
        val before = savedTokens(issuer).copy(proof = proof)
        val answer = TokenResponse("a", 60, "r", "library")
        assertEquals(proof, answer.stored(issuer, "t", "r", now, before).proof)
        // A sign-in's key never changes: an answer bringing another one doesn't replace it.
        val newer = ServerProof(SERVER_ID, "8", keyText)
        assertEquals(proof, answer.copy(proof = newer).stored(issuer, "t", "r", now, before).proof)
        // A sign-in without one gets it.
        assertEquals(newer, answer.copy(proof = newer).stored(issuer, "t", "r", now, null).proof)
    }

    @Test
    fun theServerProvesItselfOnceInHalfAnHour() = runBlocking {
        signedIn()
        val tokens = tokens()
        assertEquals("pley_at_one", tokens.accessToken())
        assertEquals("pley_at_one", tokens.accessToken())
        assertEquals(1, challenges.size)
        // A fresh challenge each time: 16 random bytes.
        assertEquals(16, Base64.getUrlDecoder().decode(challenges[0]).size)
        now += ServerProofs.TTL_MS - 1
        tokens.accessToken()
        assertEquals(1, challenges.size)
        now += 1
        tokens.accessToken()
        assertEquals(2, challenges.size)
        assertTrue(challenges[0] != challenges[1])
    }

    @Test
    fun aRefreshHasTheServerProveItselfAnew() = runBlocking {
        signedIn()
        val tokens = tokens()
        tokens.accessToken()
        assertEquals(1, challenges.size)
        tokens.refresh("pley_at_one")
        assertEquals(2, challenges.size)
    }

    @Test
    fun aTokenAnotherRefreshGaveGoesOutOnlyAfterAProof() = runBlocking {
        signedIn()
        assertEquals("pley_at_one", tokens().refresh("pley_at_old"))
        assertEquals(1, challenges.size)
        assertEquals(0, refreshes)
    }

    @Test
    fun aRequestWithoutAnAnswerHasItProveItselfAgain() = runBlocking {
        signedIn()
        val tokens = tokens()
        tokens.accessToken()
        tokens.doubtServer()
        tokens.accessToken()
        assertEquals(2, challenges.size)
    }

    @Test
    fun anotherNetworkOrAClockSetBackHasItProveItselfAgain() = runBlocking {
        signedIn()
        val tokens = tokens()
        tokens.accessToken()
        ServerProofs.networkMayHaveChanged()
        tokens.accessToken()
        assertEquals(2, challenges.size)
        now -= 60 * 60_000
        tokens.accessToken()
        assertEquals(3, challenges.size)
    }

    @Test
    fun aServerErrorIsOutOfReachNotAnotherDevice() = runBlocking {
        signedIn()
        answer = { MockResponse().setResponseCode(502) }
        val refused = assertThrows(java.io.IOException::class.java) { runBlocking { tokens().accessToken() } }
        assertFalse(refused is NotYourServerException)
    }

    @Test
    fun aNewSignInEndsTheOneBeforeOnlyWhereTheServerProvesItsKey() = runBlocking {
        val fresh = savedTokens(issuer).copy(accessToken = "pley_at_new", refreshToken = "pley_rt_new")
        val answerOf = TokenResponse("pley_at_new", 60, "pley_rt_new", fresh.scope)
        for (proves in listOf(false, true)) {
            answer = { c -> MockResponse().setBody(JSONObject().put("id", SERVER_ID).put("proof", ServerProofs.proof(if (proves) key else ByteArray(32), SERVER_ID, address, c)).toString()) }
            signedIn()
            keepNewSignIn(endpointsClient(), tokens(), answerOf, fresh.scope, fresh)
            assertEquals(if (proves) listOf("pley_rt_one") else emptyList<String>(), revokes)
        }
    }

    @Test
    fun anotherDeviceGetsNoToken() = runBlocking {
        signedIn(expiresAt = now + 30_000)
        val tokens = tokens()
        val wrong = listOf(
            // Another key.
            { c: String -> MockResponse().setBody(JSONObject().put("id", SERVER_ID).put("proof", ServerProofs.proof(ByteArray(32), SERVER_ID, address, c)).toString()) },
            // Another server's id, with the proof made for it.
            { c: String -> MockResponse().setBody(JSONObject().put("id", "other").put("proof", ServerProofs.proof(key, "other", address, c)).toString()) },
            // No proof: a server that doesn't know the challenge, or something else entirely.
            { _: String -> MockResponse().setBody(JSONObject().put("id", SERVER_ID).put("name", "x").toString()) },
            { _: String -> MockResponse().setBody("<html>router</html>") },
            { _: String -> MockResponse().setResponseCode(404) },
            { _: String -> MockResponse().setResponseCode(302).setHeader("Location", "http://127.0.0.1:1/api/server") },
        )
        for (each in wrong) {
            answer = each
            val refused = assertThrows(NotYourServerException::class.java) { runBlocking { tokens.accessToken() } }
            // Out of reach, as the app says it.
            assertEquals("another device answers there", describe(refused))
            assertThrows(NotYourServerException::class.java) { runBlocking { tokens.refresh("pley_at_one") } }
            assertThrows(NotYourServerException::class.java) { runBlocking { tokens.switchProfile(2, null) } }
        }
        // Nothing went to the token endpoint, and the sign-in is kept for when the server is back.
        assertEquals(0, refreshes)
        assertTrue(tokens.signedIn.value)
        assertEquals("pley_rt_one", store.load(issuer)!!.refreshToken)
    }

    @Test
    fun signingOutSendsNothingToAnotherDevice() = runBlocking {
        signedIn()
        answer = { MockResponse().setResponseCode(404) }
        val tokens = tokens()
        tokens.signOut()
        assertEquals(emptyList<String>(), revokes)
        assertNull(store.load(issuer))
        assertFalse(tokens.signedIn.value)

        // The server itself gets the revocation.
        answer = { c -> MockResponse().setBody(JSONObject().put("id", SERVER_ID).put("proof", ServerProofs.proof(key, SERVER_ID, address, c)).toString()) }
        signedIn()
        tokens.signOut()
        assertEquals(listOf("pley_rt_one"), revokes)
    }

    @Test
    fun aSignInFromAnOlderServerHasNothingToCheck() = runBlocking {
        store.save(savedTokens(issuer))
        assertEquals("pley_at_one", tokens().accessToken())
        assertEquals(0, challenges.size)
    }

    @Test
    fun aRefreshKeepsItsKeyAndHasTheServerProveItselfAnew() = runBlocking {
        signedIn(expiresAt = now + 30_000)
        val tokens = tokens()
        // The access token is about to end: one proof, the refresh token's.
        assertEquals("pley_at_new1", tokens.accessToken())
        assertEquals(1, challenges.size)
        assertEquals(proof, store.load(issuer)!!.proof)
        // A refresh that brings another key doesn't replace the sign-in's.
        val newer = ServerProof(SERVER_ID, "7", Base64.getUrlEncoder().withoutPadding().encodeToString(ByteArray(32) { 9 }))
        refreshAnswer = {
            JSONObject(tokenAnswer("pley_at_new${++refreshes}", "pley_rt_new$refreshes"))
                .put("server_id", newer.serverId).put("proof_id", newer.proofId).put("proof_key", newer.proofKey).toString()
        }
        tokens.refresh("pley_at_new1")
        assertEquals(proof, store.load(issuer)!!.proof)
    }

    @Test
    fun aRequestThatGetsNoAnswerHasTheServerProveItselfAgain() {
        signedIn()
        val tokens = tokens()
        val client = AuthHttp.api(endpointsClient(), tokens)
        fun get(path: String) = client.newCall(Request.Builder().url(server.url(path)).build())
        get("/api/libraries").execute().close()
        assertEquals(1, challenges.size)
        get("/api/libraries").execute().close()
        assertEquals(1, challenges.size)

        // The connection drops: the next request waits for a new proof.
        val dispatcher = server.dispatcher
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse =
                if (request.path == "/api/dropped") MockResponse().setSocketPolicy(SocketPolicy.DISCONNECT_AFTER_REQUEST) else dispatcher.dispatch(request)
        }
        assertThrows(java.io.IOException::class.java) { get("/api/dropped").execute() }
        get("/api/libraries").execute().close()
        assertEquals(2, challenges.size)

        // A request elsewhere that fails says nothing about the server.
        assertThrows(java.io.IOException::class.java) { client.newCall(Request.Builder().url("http://127.0.0.1:1/x").build()).execute() }
        get("/api/libraries").execute().close()
        assertEquals(2, challenges.size)
    }

    companion object {
        const val SERVER_ID = "0123456789abcdef0123456789abcdef01234567"
    }
}
