package app.pley.android.auth

import app.pley.android.common.Addresses
import app.pley.android.common.UiText
import app.pley.android.common.UserFacing
import javax.crypto.KeyGenerator
import okhttp3.OkHttpClient
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.RecordedRequest

/** What the screens show of [error]: the sign-in's own words. */
fun textOf(error: Throwable?): UiText? = (error as? UserFacing)?.text

/** The shelf of a test: a map, with what was written. */
class MemoryShelf : Shelf {
    val values = linkedMapOf<String, String>()

    /** Set to fail the next writes (a full disk). */
    var failWrites = false

    override fun get(name: String): String? = synchronized(values) { values[name] }

    override fun put(name: String, value: String) {
        check(!failWrites) { "disk full" }
        synchronized(values) { values[name] = value }
    }

    override fun remove(name: String) {
        synchronized(values) { values.remove(name) }
    }
}

/** A vault with a software key, as the Keystore's would be on a device. */
fun testVault(): Vault {
    val key = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
    return Vault { key }
}

/** The sign-in module's endpoint client, as the app makes it. */
fun endpointsClient(base: OkHttpClient = OkHttpClient()): OkHttpClient = AuthHttp.endpoints(userAgent = "pleyTest/1", base = base)

/** The address of [server] as the app writes it (its issuer). */
fun issuerOf(server: MockWebServer): String = Addresses.origin(server.url("/").toString())!!

/** The form parameters of [request]. */
fun formOf(request: RecordedRequest): Map<String, String> = request.body.clone().readUtf8().split('&').filter { it.isNotEmpty() }.associate {
    java.net.URLDecoder.decode(it.substringBefore('='), "UTF-8") to java.net.URLDecoder.decode(it.substringAfter('='), "UTF-8")
}

/** Tokens saved for [issuer], with an access token ending at [expiresAt]. */
fun savedTokens(
    issuer: String,
    access: String = "pley_at_one",
    refresh: String? = "pley_rt_one",
    expiresAt: Long = Long.MAX_VALUE / 2,
    scope: String = "library play playlists profiles",
) = StoredTokens(
    issuer = issuer,
    tokenEndpoint = "$issuer/api/oauth/token",
    revocationEndpoint = "$issuer/api/oauth/revoke",
    accessToken = access,
    accessExpiresAt = expiresAt,
    refreshToken = refresh,
    scope = scope,
)

fun tokenAnswer(access: String, refresh: String?, scope: String = "library play playlists profiles", expiresIn: Long = 3600): String =
    org.json.JSONObject()
        .put("access_token", access)
        .put("token_type", "Bearer")
        .put("expires_in", expiresIn)
        .put("scope", scope)
        .apply { if (refresh != null) put("refresh_token", refresh) }
        .toString()
