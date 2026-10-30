package app.pley.android.fixtures

import app.pley.android.model.AppProfile
import app.pley.android.model.Me
import app.pley.android.model.ProfileNeeds
import java.net.URI
import java.net.URLDecoder
import java.security.MessageDigest
import java.util.Base64
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Signing in as pley's authorization server does: PKCE, rotation, the device grant, profiles. */
class OAuthTest {
    private val clientId = "https://pley-media-server.github.io/pley/oauth/android.json"
    private val redirect = "app.pley.android:/oauth/callback"
    private val verifier = "a".repeat(20) + "-B_c.d~" + "9".repeat(20)
    private val challenge = Base64.getUrlEncoder().withoutPadding()
        .encodeToString(MessageDigest.getInstance("SHA-256").digest(verifier.toByteArray()))

    @Test
    fun discoveryNamesTheAddressAskedAt() = withServer { server ->
        val app = Client(server)
        val metadata = app.get("/.well-known/oauth-authorization-server")
        assertEquals("no-store", metadata.header("Cache-Control"))
        val json = metadata.json()
        assertEquals(server.url, json["issuer"]?.jsonPrimitive?.content)
        assertEquals("${server.url}/api/oauth/token", json["token_endpoint"]?.jsonPrimitive?.content)
        assertEquals("${server.url}/api/oauth/device", json["device_authorization_endpoint"]?.jsonPrimitive?.content)
        val grants = json["grant_types_supported"]!!.jsonArray.map { it.jsonPrimitive.content }
        assertEquals(
            listOf("authorization_code", "refresh_token", "urn:pley:params:oauth:grant-type:switch-profile", "urn:ietf:params:oauth:grant-type:device_code"),
            grants,
        )
        assertEquals(404, app.get("/.well-known/openid-configuration").code)
    }

    @Test
    fun signingInWithPkce() = withServer { server ->
        val app = Client(server)
        val query = signIn(app, server, scope = "library play")
        assertEquals("s1", query["state"])
        assertEquals(server.url, query["iss"])
        val code = query.getValue("code")

        // A wrong verifier leaves the code as it was.
        val wrong = app.form("/api/oauth/token", "grant_type" to "authorization_code", "code" to code, "client_id" to clientId, "code_verifier" to "x".repeat(43))
        assertEquals(400, wrong.code)
        assertEquals("invalid_grant", wrong.field("error"))

        val tokens = exchange(app, code)
        assertEquals(200, tokens.code)
        assertEquals("no-store", tokens.header("Cache-Control"))
        assertEquals("library play", tokens.field("scope"))
        assertEquals("Bearer", tokens.field("token_type"))
        val access = tokens.field("access_token")!!
        assertTrue(access.startsWith("pley_at_"))
        assertTrue(tokens.field("refresh_token")!!.startsWith("pley_rt_"))
        app.token = access
        assertEquals("library play", app.read("/api/auth/me", Me.serializer()).scope)

        // The code shown again ends what the first exchange got.
        assertEquals("invalid_grant", exchange(app, code).field("error"))
        assertEquals(401, app.get("/api/auth/me").code)
    }

    @Test
    fun theAuthorizePageSendsMistakesBackOrRefusesThem() = withServer { server ->
        val app = Client(server)
        val base = "/oauth/authorize?response_type=code&client_id=${enc(clientId)}&redirect_uri=${enc(redirect)}&state=s"
        assertEquals("invalid_request", location(app.get(base))["error"])
        assertEquals("invalid_scope", location(app.get("$base&code_challenge=$challenge&code_challenge_method=S256&scope=openid"))["error"])
        // Another app or return address is answered here, never sent anywhere.
        assertEquals(400, app.get(base.replace(enc(redirect), enc("https://evil.example/cb")) + "&code_challenge=$challenge&code_challenge_method=S256").code)
    }

    @Test
    fun aRefreshTokenShownTwiceEndsTheGrant() = withServer { server ->
        val app = Client(server)
        val first = exchange(app, signIn(app, server).getValue("code"))
        val refresh = first.field("refresh_token")!!
        val second = app.form("/api/oauth/token", "grant_type" to "refresh_token", "refresh_token" to refresh, "client_id" to clientId)
        assertEquals(200, second.code)
        val newer = second.field("refresh_token")!!
        assertNotEquals(refresh, newer)
        app.token = second.field("access_token")
        assertEquals(200, app.get("/api/auth/me").code)

        val again = app.form("/api/oauth/token", "grant_type" to "refresh_token", "refresh_token" to refresh, "client_id" to clientId)
        assertEquals("invalid_grant", again.field("error"))
        // The whole grant ended: the newer tokens too.
        assertEquals(401, app.get("/api/auth/me").code)
        assertEquals("invalid_grant", app.form("/api/oauth/token", "grant_type" to "refresh_token", "refresh_token" to newer, "client_id" to clientId).field("error"))
    }

    @Test
    fun accessTokensEndAfterAnHour() {
        val clock = FakeClock()
        withServer(FixtureServer.Options(scanEveryMs = 0, clock = clock)) { server ->
            val app = Client(server, server.accessToken())
            assertEquals(200, app.get("/api/auth/me").code)
            clock.now += 3_600_000
            val expired = app.get("/api/auth/me")
            assertEquals(401, expired.code)
            assertEquals("Bearer error=\"invalid_token\"", expired.header("WWW-Authenticate"))
        }
    }

    @Test
    fun revokingEndsTheGrant() = withServer { server ->
        val app = Client(server)
        val tokens = exchange(app, signIn(app, server).getValue("code"))
        app.token = tokens.field("access_token")
        assertEquals(200, app.form("/api/oauth/revoke", "token" to tokens.field("refresh_token")!!, "client_id" to clientId).code)
        assertEquals(401, app.get("/api/auth/me").code)
    }

    @Test
    fun theDeviceGrant() {
        val clock = FakeClock()
        withServer(FixtureServer.Options(scanEveryMs = 0, clock = clock)) { server ->
            val tv = Client(server)
            val start = tv.form("/api/oauth/device", "client_id" to clientId, "scope" to "library play profiles")
            assertEquals(200, start.code)
            val device = start.field("device_code")!!
            assertTrue(Regex("[B-Z]{4}-[B-Z]{4}").matches(start.field("user_code")!!))
            assertEquals("${server.url}/link", start.field("verification_uri"))
            assertEquals("5", start.field("interval"))
            fun poll() = tv.form("/api/oauth/token", "grant_type" to "urn:ietf:params:oauth:grant-type:device_code", "device_code" to device, "client_id" to clientId)

            assertEquals("authorization_pending", poll().field("error"))
            // Asked again too soon: wait 5 s longer.
            clock.now += 1_000
            assertEquals("slow_down", poll().field("error"))
            clock.now += 10_000
            val tokens = poll()
            assertEquals(200, tokens.code)
            assertEquals("library play profiles", tokens.field("scope"))
            // The code works once.
            clock.now += 10_000
            assertEquals("invalid_grant", poll().field("error"))
        }
    }

    @Test
    fun aRefusedOrExpiredDeviceCode() {
        val clock = FakeClock()
        withServer(FixtureServer.Options(scanEveryMs = 0, clock = clock)) { server ->
            val tv = Client(server)
            fun start() = tv.form("/api/oauth/device", "client_id" to clientId).field("device_code")!!
            fun poll(code: String) = tv.form("/api/oauth/token", "grant_type" to "urn:ietf:params:oauth:grant-type:device_code", "device_code" to code, "client_id" to clientId).field("error")
            server.deviceApproval = FixtureServer.DeviceApproval.Deny
            val denied = start()
            assertEquals("authorization_pending", poll(denied))
            clock.now += 6_000
            assertEquals("access_denied", poll(denied))
            val late = start()
            clock.now += 601_000
            assertEquals("expired_token", poll(late))
        }
    }

    @Test
    fun switchingProfilesWithAPin() = withServer { server ->
        val app = Client(server)
        val tokens = exchange(app, signIn(app, server).getValue("code"))
        app.token = tokens.field("access_token")
        var refresh = tokens.field("refresh_token")!!
        val profiles = app.read("/api/oauth/profiles", ListSerializer(AppProfile.serializer()))
        assertEquals(
            listOf(ProfileNeeds.None, ProfileNeeds.Pin, ProfileNeeds.Password),
            profiles.map { it.needs },
        )
        assertEquals(FixtureIds.ALEX, profiles.single { it.current }.id)
        fun switch(profile: Long, pin: String? = null): Answer {
            val fields = listOfNotNull(
                "grant_type" to "urn:pley:params:oauth:grant-type:switch-profile",
                "client_id" to clientId,
                "refresh_token" to refresh,
                "profile" to profile.toString(),
                pin?.let { "pin" to it },
            )
            return app.form("/api/oauth/token", *fields.toTypedArray())
        }

        // A refusal spends nothing.
        assertEquals("wrong_pin", switch(FixtureIds.SAM).field("reason"))
        assertEquals("wrong_pin", switch(FixtureIds.SAM, "0000").field("reason"))
        assertEquals("browser", switch(FixtureIds.ROBIN).field("reason"))
        assertEquals("invalid_request", switch(FixtureIds.ALEX).field("error"))

        val sam = switch(FixtureIds.SAM, FixtureIds.PIN)
        assertEquals(200, sam.code)
        // The old access token stopped at once.
        assertEquals(401, app.get("/api/auth/me").code)
        app.token = sam.field("access_token")
        refresh = sam.field("refresh_token")!!
        assertEquals(FixtureIds.SAM, app.read("/api/auth/me", Me.serializer()).id)

        // To oneself with the PIN: checked, and the tokens rotate.
        assertEquals("invalid_request", switch(FixtureIds.SAM).field("error"))
        assertEquals("wrong_pin", switch(FixtureIds.SAM, "1111").field("reason"))
        val same = switch(FixtureIds.SAM, FixtureIds.PIN)
        assertEquals(200, same.code)
        val old = refresh
        refresh = same.field("refresh_token")!!
        assertNotEquals(old, refresh)
    }

    @Test
    fun fiveWrongPinsThrottle() = withServer { server ->
        val app = Client(server)
        val refresh = exchange(app, signIn(app, server).getValue("code")).field("refresh_token")!!
        fun wrong() = app.form(
            "/api/oauth/token",
            "grant_type" to "urn:pley:params:oauth:grant-type:switch-profile",
            "client_id" to clientId,
            "refresh_token" to refresh,
            "profile" to FixtureIds.SAM.toString(),
            "pin" to "9999",
        ).field("reason")
        repeat(5) { assertEquals("wrong_pin", wrong()) }
        assertEquals("throttled", wrong())
    }

    @Test
    fun routesCheckTheTokenAndItsScopes() = withServer { server ->
        val none = Client(server)
        val missing = none.get("/api/libraries")
        assertEquals(401, missing.code)
        val library = Client(server, server.accessToken(scopes = setOf("library")))
        assertEquals(200, library.get("/api/libraries").code)
        val refused = library.send("POST", "/api/playback", """{"item_id":1,"caps":{}}""")
        assertEquals(403, refused.code)
        assertEquals("insufficient_scope", refused.field("code"))
        assertEquals("Bearer error=\"insufficient_scope\", scope=\"play\"", refused.header("WWW-Authenticate"))
        assertEquals("Bearer error=\"insufficient_scope\"", library.get("/api/activity").header("WWW-Authenticate"))
        assertEquals(403, library.get("/api/oauth/profiles").code)
        // Apps' tokens go in the header only.
        val inQuery = none.get("/api/libraries?token=${server.accessToken()}")
        assertEquals(400, inQuery.code)
        assertEquals("Bearer error=\"invalid_request\"", inQuery.header("WWW-Authenticate"))
    }

    @Test
    fun withoutRequireAuthEveryRouteIsOpen() = withServer(FixtureServer.Options(requireAuth = false, scanEveryMs = 0)) { server ->
        assertEquals(200, Client(server).get("/api/playlists").code)
    }

    // ---------------------------------------------------------------- helpers

    /** Opens the authorize page as the app's browser would; the callback's query. */
    private fun signIn(app: Client, server: FixtureServer, scope: String = "library play playlists profiles"): Map<String, String> {
        val page = app.get(
            "/oauth/authorize?response_type=code&client_id=${enc(clientId)}&redirect_uri=${enc(redirect)}" +
                "&code_challenge=$challenge&code_challenge_method=S256&scope=${enc(scope)}&state=s1",
        )
        assertEquals(page.text, 302, page.code)
        assertTrue(page.header("Location")!!.startsWith("$redirect?"))
        assertEquals(server.url, location(page)["iss"])
        return location(page)
    }

    private fun exchange(app: Client, code: String) = app.form(
        "/api/oauth/token",
        "grant_type" to "authorization_code",
        "code" to code,
        "client_id" to clientId,
        "redirect_uri" to redirect,
        "code_verifier" to verifier,
    )

    private fun location(answer: Answer): Map<String, String> {
        val location = answer.header("Location") ?: error("no Location: ${answer.code} ${answer.text}")
        val query = URI(location).rawQuery.orEmpty()
        return query.split('&').filter { it.isNotEmpty() }.associate {
            URLDecoder.decode(it.substringBefore('='), "UTF-8") to URLDecoder.decode(it.substringAfter('='), "UTF-8")
        }
    }

    private fun enc(s: String) = java.net.URLEncoder.encode(s, "UTF-8")
}
