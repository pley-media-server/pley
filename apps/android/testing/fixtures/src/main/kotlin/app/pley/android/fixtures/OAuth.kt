package app.pley.android.fixtures

import app.pley.android.model.AppProfile
import app.pley.android.model.ProfileNeeds
import java.net.URLEncoder
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Base64
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import mockwebserver3.MockResponse

/**
 * pley's authorization server as the app meets it (docs/API.md, "Apps
 * (OAuth 2.1)", with the device grant of RFC 8628): discovery with the
 * issuer from Host, the authorize page (which allows at once, as
 * [FixtureServer.signInAs]), the token endpoint's four grants with refresh
 * rotation and reuse detection, revocation, the household's profiles, and
 * every route's token check. Only pley for Android's client ID is known.
 */
internal class OAuth(private val state: State) {
    class Grant(val id: Long, var profileId: Long, val scopes: Set<String>) {
        var ended = false

        /** Its access tokens and when each ends (ms), oldest first. */
        val access = LinkedHashMap<String, Long>()
        var refresh: String? = null
    }

    private class RefreshToken(val grant: Grant) {
        var spent = false
    }

    private class Code(
        val challenge: String,
        val redirectUri: String,
        val scopes: Set<String>,
        val profileId: Long,
        val expiresAt: Long,
    ) {
        var used: Grant? = null
    }

    private class Device(val userCode: String, val scopes: Set<String>, val expiresAt: Long, var interval: Int) {
        var lastPoll: Long? = null
        var polls = 0
    }

    private val random = SecureRandom()
    private var nextGrant = 1L
    private val accessTokens = HashMap<String, Grant>()
    private val refreshTokens = HashMap<String, RefreshToken>()
    private val codes = HashMap<String, Code>()
    private val devices = HashMap<String, Device>()

    /** The server's id, as `/api/server` and the token answers give it. */
    val serverId = "f1c7e5a0d2b94c6e8a3f7d1b5c9e2a4f6b8d0c13"

    /**
     * What the grants' proof keys come from, as a server's secret.key: the
     * same in every process, since a server keeps it when it restarts (and
     * the demo server restarts with the app, which keeps its tokens).
     */
    private val proofRoot = MessageDigest.getInstance("SHA-256").digest("pley fixture server".toByteArray(Charsets.UTF_8))

    /** The profile the authorize page and the device grant sign in as. */
    var signInAs: Long = FixtureIds.ALEX

    /** How the person answers a TV's code. */
    var deviceApproval: FixtureServer.DeviceApproval = FixtureServer.DeviceApproval.Allow

    /**
     * The demo server only: tokens this process never issued are taken as
     * [signInAs]'s, with every scope. The demo server lives in the app's
     * process, so it forgets its grants when the app restarts while the app
     * keeps its tokens; this keeps a device signed in to it. Tests leave it off.
     */
    var adoptUnknownTokens: Boolean = false

    /**
     * Every access token is refused with invalid_token while refreshes still
     * answer: a server whose token check no longer accepts this sign-in.
     */
    var refuseAccessTokens: Boolean = false

    /** A grant for a token this process never issued ([adoptUnknownTokens]). */
    private fun adopt(): Grant = Grant(nextGrant++, signInAs, SCOPES.toSet())

    // ---------------------------------------------------------------- the token check of every route

    /**
     * Who [call] comes from, by its access token; or the refusal: a token
     * outside the Authorization header, none, or one that doesn't work.
     */
    fun authenticate(call: Call): Pair<Caller?, MockResponse?> {
        if (call.query("token") != null || call.header("X-Pley-Token") != null) {
            return null to Reply.error(400, "bad_request", "an app's token goes in the Authorization header only", "Bearer error=\"invalid_request\"")
        }
        val header = call.header("Authorization")
        if (header == null) {
            if (!state.options.requireAuth) {
                val profile = state.profile(signInAs)!!
                return Caller(profile, SCOPES.toSet(), null, null) to null
            }
            return null to Reply.error(401, "unauthorized", "sign in first", "Bearer")
        }
        val token = header.removePrefix("Bearer ").trim()
        if (adoptUnknownTokens && token.startsWith("pley_at_") && token !in accessTokens) {
            val adopted = adopt()
            adopted.access[token] = state.nowMs() + ACCESS_LIFETIME_S * 1_000
            accessTokens[token] = adopted
        }
        val grant = accessTokens[token]
        val expiresAt = grant?.access?.get(token)
        if (refuseAccessTokens || !header.startsWith("Bearer ") || grant == null || grant.ended || expiresAt == null || expiresAt <= state.nowMs()) {
            return null to Reply.error(401, "unauthorized", "the token doesn't work: refresh it or sign in again", "Bearer error=\"invalid_token\"")
        }
        return Caller(state.profile(grant.profileId)!!, grant.scopes, grant, expiresAt) to null
    }

    /** A grant for [profile] with [scopes], as if the app had signed in; its access token. */
    fun grantFor(profile: Long, scopes: Set<String>): String {
        val grant = Grant(nextGrant++, profile, scopes)
        issue(grant)
        return grant.access.keys.last()
    }

    // ---------------------------------------------------------------- routes

    fun routes(): List<Route> = listOf(
        public("GET", "/.well-known/oauth-authorization-server") { call, _ -> metadata(call) },
        public("GET", "/.well-known/*") { _, _ -> Reply.notFound("no such document") },
        public("GET", "/oauth/authorize") { call, _ -> authorize(call) },
        public("POST", "/api/oauth/token") { call, _ -> token(call) },
        public("POST", "/api/oauth/revoke") { call, _ -> revoke(call) },
        public("POST", "/api/oauth/device") { call, _ -> deviceAuthorization(call) },
        route("GET", "/api/oauth/profiles", Need.Scope("profiles")) { _, caller, _ -> profiles(caller) },
        route("POST", "/api/auth/logout", Need.AnyApp) { _, caller, _ ->
            caller.grant?.let(::end)
            Reply.noContent()
        },
    )

    /** The issuer: the address the request came to (docs/API.md, "Discovery"). */
    private fun issuer(call: Call): String? {
        val host = call.header("Host")?.lowercase() ?: return null
        if ('/' in host || '@' in host) return null
        val https = call.header("X-Forwarded-Proto")?.substringBefore(',')?.trim() == "https"
        val plain = if (https) host.removeSuffix(":443") else host.removeSuffix(":80")
        return (if (https) "https://" else "http://") + plain
    }

    private fun metadata(call: Call): MockResponse {
        val issuer = issuer(call) ?: return Reply.badRequest("the Host header is missing or wrong")
        val body = buildJsonObject {
            put("issuer", issuer)
            put("authorization_endpoint", "$issuer/oauth/authorize")
            put("token_endpoint", "$issuer/api/oauth/token")
            put("revocation_endpoint", "$issuer/api/oauth/revoke")
            put("device_authorization_endpoint", "$issuer/api/oauth/device")
            put("response_types_supported", strings("code"))
            put("response_modes_supported", strings("query"))
            put("grant_types_supported", strings("authorization_code", "refresh_token", SWITCH_PROFILE, DEVICE_GRANT))
            put("token_endpoint_auth_methods_supported", strings("none"))
            put("revocation_endpoint_auth_methods_supported", strings("none"))
            put("code_challenge_methods_supported", strings("S256"))
            put("scopes_supported", strings(*SCOPES.toTypedArray()))
            put("authorization_response_iss_parameter_supported", true)
            put("client_id_metadata_document_supported", false)
            put("service_documentation", "https://github.com/pley-media-server/pley/blob/master/docs/API.md")
        }
        return Reply.json(body.toString(), headers = mapOf("Cache-Control" to "no-store"))
    }

    // ---------------------------------------------------------------- the authorize page

    /**
     * GET /oauth/authorize: the page that asks the person, which here
     * allows at once, as [signInAs], and sends the browser back to the app
     * with a 302. An unknown app or return address is answered here and
     * never sent to the app.
     */
    private fun authorize(call: Call): MockResponse {
        val issuer = issuer(call) ?: return Reply.badRequest("the Host header is missing or wrong")
        if (call.query("client_id") != CLIENT_ID) return Reply.badRequest("Pley doesn't know this app")
        val redirect = call.query("redirect_uri")
        if (redirect != REDIRECT_URI) return Reply.badRequest("this isn't one of the app's return addresses")
        val sentState = call.query("state")
        fun back(vararg answer: Pair<String, String?>): MockResponse {
            val all = answer.toList() + ("state" to sentState) + ("iss" to issuer)
            val query = all.filter { it.second != null }.joinToString("&") { (name, value) -> "$name=${encode(value!!)}" }
            return MockResponse.Builder().code(302).setHeader("Location", "$redirect?$query").build()
        }
        fun mistake(error: String, description: String) = back("error" to error, "error_description" to description)

        if (call.query("request") != null || call.query("request_uri") != null) return mistake("invalid_request", "request objects aren't taken")
        if (call.query("response_type") != "code") return mistake("unsupported_response_type", "response_type must be code")
        val mode = call.query("response_mode")
        if (mode != null && mode != "query") return mistake("invalid_request", "response_mode must be query")
        val challenge = call.query("code_challenge")
        if (challenge == null || challenge.length != 43 || call.query("code_challenge_method") != "S256") {
            return mistake("invalid_request", "a code_challenge with S256 is needed")
        }
        if ((sentState?.toByteArray()?.size ?: 0) > 1_024) return mistake("invalid_request", "state is too long")
        val scopes = scopes(call.query("scope")) ?: return mistake("invalid_scope", "Pley doesn't offer that scope")
        val resource = call.query("resource")
        if (resource != null && resource.removeSuffix("/") != issuer) return mistake("invalid_target", "resource names another server")
        val code = token("")
        codes[code] = Code(challenge, redirect, scopes, signInAs, state.now() + CODE_LIFETIME_S)
        return back("code" to code)
    }

    // ---------------------------------------------------------------- the token endpoint

    private fun token(call: Call): MockResponse {
        val form = form(call) ?: return error("invalid_request", "each parameter may be given once")
        if (form.containsKey("client_secret") || call.header("Authorization")?.startsWith("Basic ") == true) {
            return error("invalid_client", "Pley's apps have no secrets", status = 401, basic = call.header("Authorization") != null)
        }
        return when (form["grant_type"]) {
            null -> error("invalid_request", "grant_type is missing")
            "authorization_code" -> codeGrant(form)
            "refresh_token" -> refreshGrant(form)
            SWITCH_PROFILE -> switchProfile(form)
            DEVICE_GRANT -> deviceGrant(form)
            else -> error("unsupported_grant_type", "Pley doesn't offer that grant")
        }
    }

    private fun codeGrant(form: Map<String, String>): MockResponse {
        val code = form["code"] ?: return error("invalid_request", "code is missing")
        if (form["client_id"] == null) return error("invalid_request", "client_id is missing")
        val verifier = form["code_verifier"] ?: return error("invalid_request", "code_verifier is missing")
        val entry = codes[code]
        if (entry == null || entry.expiresAt < state.now() || form["client_id"] != CLIENT_ID) {
            return error("invalid_grant", "unknown or expired code")
        }
        val redirect = form["redirect_uri"]
        if (redirect != null && redirect != entry.redirectUri) return error("invalid_grant", "the code went to another address")
        if (!VERIFIER.matches(verifier) || pkce(verifier) != entry.challenge) return error("invalid_grant", "the code_verifier doesn't match")
        entry.used?.let { first ->
            // Someone else has the code: the access the first exchange got ends.
            end(first)
            return error("invalid_grant", "the code was used already")
        }
        val grant = Grant(nextGrant++, entry.profileId, entry.scopes)
        entry.used = grant
        return issue(grant)
    }

    private fun refreshGrant(form: Map<String, String>): MockResponse {
        if (form["client_id"] != CLIENT_ID) return error("invalid_grant", "unknown refresh token")
        val grant = spend(form["refresh_token"]) ?: return spent()
        val asked = form["scope"]?.let { scopes(it) ?: return error("invalid_scope", "Pley doesn't offer that scope") }
        if (asked != null && !grant.scopes.containsAll(asked)) return error("invalid_scope", "more than the app was allowed")
        return issue(grant)
    }

    /**
     * The switch-profile grant: to another profile of the household (its PIN
     * when it has one), or to the app's own profile with its PIN, which
     * checks the PIN and rotates the tokens (S3-APP-ROUTES); or, with a
     * cast's `ticket`, to the profile of the remote that cast, in place of
     * its PIN (`ticket_spent` once spent, expired, or not this app's for
     * this switch). A refusal spends nothing else.
     */
    private fun switchProfile(form: Map<String, String>): MockResponse {
        if (form["client_id"] != CLIENT_ID) return error("unauthorized_client", "only Pley's own apps switch profiles")
        val token = refreshTokens[form["refresh_token"]]
        if (token == null || token.spent || token.grant.ended) {
            token?.takeIf { it.spent }?.grant?.let(::end)
            return spent()
        }
        val grant = token.grant
        if ("profiles" !in grant.scopes) return error("unauthorized_client", "this app wasn't allowed to switch profiles")
        val target = form["profile"]?.toLongOrNull() ?: return error("invalid_request", "profile is missing or not an id")
        val ticket = form["ticket"]
        if (ticket != null) {
            // A cast from another profile's remote: this device switches to it.
            if (!state.events.redeem(ticket, "app-${grant.id}", grant.profileId, target, state.nowMs())) {
                return error("invalid_grant", "this cast's switch has expired or was used: cast again", reason = "ticket_spent")
            }
            if (state.profile(target) == null) return error("invalid_grant", "not a profile of the household", reason = "not_allowed")
            return rotate(token, target)
        }
        val profile = state.profile(target) ?: return error("invalid_grant", "not a profile of the household", reason = "not_allowed")
        val pin = form["pin"]
        if (target == grant.profileId && (pin == null || profile.pin == null)) {
            return error("invalid_request", "the app uses this profile already")
        }
        if (profile.pin != null) {
            val now = state.now()
            profile.wrongPins.removeAll { it < now - 15 * 60 }
            if (profile.wrongPins.size >= 5) return error("invalid_grant", "too many wrong PINs: try again in a few minutes", reason = "throttled")
            if (pin != profile.pin) {
                profile.wrongPins += now
                return error("invalid_grant", "wrong PIN", reason = "wrong_pin")
            }
        } else if (profile.needs == ProfileNeeds.Password) {
            return error("invalid_grant", "this profile needs its password: sign in again", reason = "browser")
        }
        return rotate(token, target)
    }

    /** [token]'s grant acts as profile [target] from now on, with new tokens: the old ones stop at once. */
    private fun rotate(token: RefreshToken, target: Long): MockResponse {
        val grant = token.grant
        token.spent = true
        grant.access.keys.forEach(accessTokens::remove)
        grant.access.clear()
        grant.profileId = target
        return issue(grant)
    }

    private fun deviceGrant(form: Map<String, String>): MockResponse {
        val code = form["device_code"] ?: return error("invalid_request", "device_code is missing")
        if (form["client_id"] == null) return error("invalid_request", "client_id is missing")
        val device = devices[code]
        if (device == null || form["client_id"] != CLIENT_ID) return error("invalid_grant", "unknown device_code")
        val now = state.nowMs()
        if (device.expiresAt <= now) {
            devices.remove(code)
            return error("expired_token", "the code expired: start again")
        }
        val tooSoon = device.lastPoll?.let { now - it < device.interval * 1_000L } ?: false
        device.lastPoll = now
        if (tooSoon) {
            device.interval += 5
            return error("slow_down", "asked too soon: wait 5 seconds longer between questions")
        }
        device.polls++
        if (device.polls == 1 || deviceApproval == FixtureServer.DeviceApproval.Never) {
            return error("authorization_pending", "the person hasn't answered yet")
        }
        devices.remove(code)
        if (deviceApproval == FixtureServer.DeviceApproval.Deny) return error("access_denied", "the person didn't allow this app")
        return issue(Grant(nextGrant++, signInAs, device.scopes))
    }

    /** POST /api/oauth/device: a TV starts signing in with a code. */
    private fun deviceAuthorization(call: Call): MockResponse {
        val form = form(call) ?: return error("invalid_request", "each parameter may be given once")
        val client = form["client_id"] ?: return error("invalid_request", "client_id is missing")
        if (client != CLIENT_ID) return error("invalid_client", "only Pley's own apps can sign in with a code")
        val scopes = scopes(form["scope"]) ?: return error("invalid_scope", "Pley doesn't offer that scope")
        val issuer = issuer(call) ?: return error("invalid_request", "the Host header is missing or wrong")
        val deviceCode = token("")
        val letters = (1..8).map { USER_CODE_LETTERS[random.nextInt(USER_CODE_LETTERS.length)] }.joinToString("")
        devices[deviceCode] = Device(letters, scopes, state.nowMs() + DEVICE_LIFETIME_S * 1_000, state.options.deviceIntervalSeconds)
        val body = buildJsonObject {
            put("device_code", deviceCode)
            put("user_code", letters.substring(0, 4) + "-" + letters.substring(4))
            put("verification_uri", "$issuer/link")
            put("verification_uri_complete", "$issuer/link?code=$letters")
            put("expires_in", DEVICE_LIFETIME_S)
            put("interval", state.options.deviceIntervalSeconds)
        }
        return Reply.json(body.toString(), headers = NO_STORE)
    }

    private fun revoke(call: Call): MockResponse {
        val form = form(call) ?: return error("invalid_request", "each parameter may be given once")
        val token = form["token"] ?: return error("invalid_request", "token is missing")
        if (form["client_id"] == null) return error("invalid_request", "client_id is missing")
        (accessTokens[token] ?: refreshTokens[token]?.grant)?.let(::end)
        return MockResponse.Builder().code(200).apply { NO_STORE.forEach { (k, v) -> setHeader(k, v) } }.build()
    }

    private fun profiles(caller: Caller): MockResponse {
        val list = state.household.profiles.values.map { profile ->
            val current = profile.id == caller.profile.id
            AppProfile(
                id = profile.id,
                name = profile.name,
                avatarUrl = profile.avatarUrl,
                avatarColor = profile.avatarColor,
                current = current,
                needs = if (current) ProfileNeeds.None else profile.needs,
            )
        }
        return Reply.json(ListSerializer(AppProfile.serializer()), list)
    }

    // ---------------------------------------------------------------- tokens

    /** A new token pair for [grant]; the refresh token before it is spent. */
    private fun issue(grant: Grant): MockResponse {
        val access = "pley_at_" + token("")
        grant.access[access] = state.nowMs() + ACCESS_LIFETIME_S * 1_000
        accessTokens[access] = grant
        // A grant keeps its 10 newest access tokens.
        while (grant.access.size > 10) {
            val oldest = grant.access.keys.first()
            grant.access.remove(oldest)
            accessTokens.remove(oldest)
        }
        grant.refresh?.let { refreshTokens[it]?.spent = true }
        val refresh = "pley_rt_" + token("")
        refreshTokens[refresh] = RefreshToken(grant)
        grant.refresh = refresh
        val body = buildJsonObject {
            put("access_token", access)
            put("token_type", "Bearer")
            put("expires_in", ACCESS_LIFETIME_S)
            put("scope", SCOPES.filter { it in grant.scopes }.joinToString(" "))
            put("refresh_token", refresh)
            put("server_id", serverId)
            put("proof_id", grant.id.toString())
            put("proof_key", base64url(proofKey(grant.id)))
        }
        return Reply.json(body.toString(), headers = NO_STORE)
    }

    /**
     * The grant of [refreshToken], which it spends; `null` when it doesn't
     * work. One shown again ends its grant.
     */
    private fun spend(refreshToken: String?): Grant? {
        if (adoptUnknownTokens && refreshToken != null && refreshToken.startsWith("pley_rt_") && refreshToken !in refreshTokens) {
            val adopted = adopt()
            adopted.refresh = refreshToken
            refreshTokens[refreshToken] = RefreshToken(adopted)
        }
        val token = refreshTokens[refreshToken] ?: return null
        if (token.spent) {
            end(token.grant)
            return null
        }
        if (token.grant.ended) return null
        token.spent = true
        return token.grant
    }

    private fun spent() = error("invalid_grant", "refresh token used again or unknown: the app's access is ended")

    private fun end(grant: Grant) {
        grant.ended = true
        grant.access.keys.forEach(accessTokens::remove)
        grant.access.clear()
    }

    // ---------------------------------------------------------------- helpers

    private fun error(error: String, description: String, reason: String? = null, status: Int = 400, basic: Boolean = false): MockResponse {
        val body = buildJsonObject {
            put("error", error)
            put("error_description", description)
            if (reason != null) put("reason", reason)
        }
        val headers = if (basic) NO_STORE + ("WWW-Authenticate" to "Basic realm=\"pley\"") else NO_STORE
        return Reply.json(body.toString(), status, headers)
    }

    /** The form's values, or `null` when a parameter is given twice. */
    private fun form(call: Call): Map<String, String>? {
        val form = call.form()
        if (form.values.any { it.size > 1 }) return null
        return form.mapValues { it.value.single() }
    }

    /** Asked scopes; none is `library`. `null` when one isn't pley's. */
    private fun scopes(asked: String?): Set<String>? {
        val names = asked.orEmpty().split(' ').filter { it.isNotEmpty() }.toSet()
        if (names.any { it !in SCOPES }) return null
        return names.ifEmpty { setOf("library") }
    }

    // ---------------------------------------------------------------- proofs

    /** The proof key of grant [grantId], the same at every refresh (docs/API.md, "Telling servers apart"). */
    private fun proofKey(grantId: Long): ByteArray = hmac(proofRoot, "grant $grantId")

    /**
     * `/api/server`'s `proof` of [challenge] under grant [proofId]'s key, for
     * the address [call] came to; `null` for one that isn't written as it
     * should be (a 400).
     */
    fun proof(call: Call, challenge: String, proofId: String): String? {
        if (!CHALLENGE.matches(challenge)) return null
        val grantId = proofId.takeIf { PROOF_ID.matches(it) }?.toLongOrNull() ?: return null
        val address = issuer(call) ?: return null
        return base64url(hmac(proofKey(grantId), "pley-server-proof\n$serverId\n$address\n$challenge"))
    }

    private fun hmac(key: ByteArray, text: String): ByteArray =
        Mac.getInstance("HmacSHA256").apply { init(SecretKeySpec(key, "HmacSHA256")) }.doFinal(text.toByteArray(Charsets.UTF_8))

    private fun base64url(bytes: ByteArray): String = Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)

    private fun token(prefix: String): String {
        val bytes = ByteArray(32)
        random.nextBytes(bytes)
        return prefix + Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)
    }

    private fun strings(vararg values: String) = buildJsonArray { values.forEach { add(JsonPrimitive(it)) } }

    companion object {
        const val CLIENT_ID = "https://pley-media-server.github.io/pley/oauth/android.json"
        const val REDIRECT_URI = "app.pley.android:/oauth/callback"
        const val SWITCH_PROFILE = "urn:pley:params:oauth:grant-type:switch-profile"
        const val DEVICE_GRANT = "urn:ietf:params:oauth:grant-type:device_code"

        private val CHALLENGE = Regex("^[A-Za-z0-9_-]{22,86}$")
        private val PROOF_ID = Regex("^[1-9][0-9]{0,18}$")

        /** pley's scopes, in the order the server writes them. */
        val SCOPES = listOf("library", "play", "playlists", "profiles")

        const val ACCESS_LIFETIME_S = 3_600L
        const val CODE_LIFETIME_S = 60L
        const val DEVICE_LIFETIME_S = 600L

        /** RFC 8628 §6.1's consonants. */
        private const val USER_CODE_LETTERS = "BCDFGHJKLMNPQRSTVWXZ"
        private val VERIFIER = Regex("[A-Za-z0-9._~-]{43,128}")
        private val NO_STORE = mapOf("Cache-Control" to "no-store", "Pragma" to "no-cache")

        /** BASE64URL(SHA-256(verifier)), the S256 challenge of RFC 7636. */
        fun pkce(verifier: String): String = Base64.getUrlEncoder().withoutPadding()
            .encodeToString(MessageDigest.getInstance("SHA-256").digest(verifier.toByteArray(Charsets.US_ASCII)))

        private fun encode(value: String): String = URLEncoder.encode(value, Charsets.UTF_8.name()).replace("+", "%20")
    }
}
