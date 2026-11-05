package app.pley.android.auth

import app.pley.android.common.UiText
import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.json.JSONArray
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class ServerMetadataTest {
    private val server = MockWebServer()

    @After
    fun stop() = server.shutdown()

    private fun metadata(issuer: String, change: JSONObject.() -> Unit = {}): String = JSONObject()
        .put("issuer", issuer)
        .put("authorization_endpoint", "$issuer/oauth/authorize")
        .put("token_endpoint", "$issuer/api/oauth/token")
        .put("revocation_endpoint", "$issuer/api/oauth/revoke")
        .put("response_types_supported", JSONArray(listOf("code")))
        .put("code_challenge_methods_supported", JSONArray(listOf("S256")))
        .put("scopes_supported", JSONArray(listOf("library", "play", "playlists")))
        .put("authorization_response_iss_parameter_supported", true)
        .apply(change)
        .toString()

    private fun refused(text: String, at: String = "http://192.168.1.20:7539"): UiText =
        assertThrows(AuthException::class.java) { ServerMetadata.parse(text, at) }.text

    @Test
    fun theServersOwnMetadataIsKept() {
        val m = ServerMetadata.parse(metadata("http://192.168.1.20:7539"), "http://192.168.1.20:7539/")
        assertEquals("http://192.168.1.20:7539", m.issuer)
        assertEquals("http://192.168.1.20:7539/oauth/authorize", m.authorizationEndpoint)
        assertEquals("http://192.168.1.20:7539/api/oauth/token", m.tokenEndpoint)
        assertEquals("http://192.168.1.20:7539/api/oauth/revoke", m.revocationEndpoint)
        assertEquals(listOf("library", "play", "playlists"), m.scopesSupported)
        // Default ports and upper case, as browsers and the server write addresses.
        assertEquals("https://pley.example.com", ServerMetadata.parse(metadata("https://pley.example.com"), "https://PLEY.example.com:443").issuer)
    }

    @Test
    fun theIssuerMustBeTheAddressUsed() {
        assertEquals(UiText.res(R.string.auth_other_issuer, "http://192.168.1.21:7539"), refused(metadata("http://192.168.1.21:7539")))
        refused(metadata("https://192.168.1.20:7539"))
        refused(metadata("http://192.168.1.20:7539/"))
        refused(metadata("http://192.168.1.20:7539/pley"))
    }

    @Test
    fun s256IsRequired() {
        assertEquals(TOO_OLD, refused(metadata("http://192.168.1.20:7539") { put("code_challenge_methods_supported", JSONArray(listOf("plain"))) }))
        assertEquals(TOO_OLD, refused(metadata("http://192.168.1.20:7539") { remove("code_challenge_methods_supported") }))
    }

    @Test
    fun everyEndpointIsOnTheIssuersAddress() {
        val issuer = "http://192.168.1.20:7539"
        for (name in listOf("authorization_endpoint", "token_endpoint", "revocation_endpoint")) {
            for (elsewhere in listOf("https://evil.example.com/token", "http://192.168.1.20:8080/x", "https://192.168.1.20:7539/x", "http://192.168.1.20:7539@evil.example.com/x")) {
                refused(metadata(issuer) { put(name, elsewhere) })
            }
            assertEquals(TOO_OLD, refused(metadata(issuer) { remove(name) }))
        }
    }

    @Test
    fun olderServersAreTooOld() = runBlocking {
        val http = endpointsClient()
        server.enqueue(MockResponse().setResponseCode(404).setBody("{\"code\":\"not_found\"}").setHeader("Content-Type", "application/json"))
        server.enqueue(MockResponse().setBody("<!doctype html><title>pley</title>").setHeader("Content-Type", "text/html; charset=utf-8"))
        server.enqueue(MockResponse().setBody("not json").setHeader("Content-Type", "application/json"))
        repeat(3) {
            val e = runCatching { ServerMetadata.fetch(http, server.url("/").toString()) }.exceptionOrNull()
            assertEquals(TOO_OLD, textOf(e))
        }
        assertEquals("/.well-known/oauth-authorization-server", server.takeRequest().path)
    }

    @Test
    fun metadataIsReadWithoutRedirects() = runBlocking {
        val issuer = issuerOf(server)
        server.enqueue(MockResponse().setResponseCode(302).setHeader("Location", "https://elsewhere.example.com/.well-known/oauth-authorization-server"))
        val e = runCatching { ServerMetadata.fetch(endpointsClient(), issuer) }.exceptionOrNull()
        assertTrue(e is AuthException)
        assertEquals(1, server.requestCount)

        server.enqueue(MockResponse().setBody(metadata(issuer)).setHeader("Content-Type", "application/json"))
        val m = ServerMetadata.fetch(endpointsClient(), issuer)
        assertEquals(issuer, m.issuer)
        val request = server.takeRequest().let { server.takeRequest() }
        assertEquals("pleyTest/1", request.getHeader("User-Agent"))
        assertNull("no cookies, no tokens", request.getHeader("Cookie") ?: request.getHeader("Authorization"))
    }
}
