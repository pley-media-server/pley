package app.pley.android.auth

import app.pley.android.common.Addresses
import app.pley.android.common.UiText
import java.io.IOException
import java.net.URI
import java.net.URISyntaxException
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import okhttp3.OkHttpClient
import org.json.JSONException
import org.json.JSONObject

/** RFC 8628's grant: a TV collects its tokens with the code it was given. */
const val DEVICE_CODE_GRANT = "urn:ietf:params:oauth:grant-type:device_code"

/** What a server without sign-in with a code answers. */
val NO_CODES: UiText = UiText.res(R.string.auth_no_codes)

/**
 * A code for signing in a TV (RFC 8628): shown as [userCode] with
 * [verificationUri] and a QR code of [verificationUriComplete], and
 * approved on a phone or computer at the server's /link page.
 */
data class DeviceCode(
    val server: String,
    /** What the TV polls with. Never shown. */
    val deviceCode: String,
    /** As shown: `BCDF-GHJK`. */
    val userCode: String,
    /** `http://192.168.1.20:7539/link`, the address the TV used, with its port. */
    val verificationUri: String,
    /** The same with the code in it, for the QR code. */
    val verificationUriComplete: String?,
    /** When the code stops working, in milliseconds since the epoch. */
    val expiresAt: Long,
    /** How often to ask, in seconds; the server's own interval. */
    val intervalSeconds: Int,
    /** The server's name for itself (its metadata's issuer), under which the tokens are kept. */
    val issuer: String = server,
    /** Where the TV asks, and where the tokens it replaces are ended: the metadata's, pley's own paths by default. */
    val tokenEndpoint: String = "$issuer/api/oauth/token",
    val revocationEndpoint: String = "$issuer/api/oauth/revoke",
    /** What the TV asked for: an answer with more is refused (ScopeGuard). */
    val scope: String = SCOPES.joinToString(" "),
) {
    // Never print the device code.
    override fun toString(): String = "DeviceCode(server=$server, userCode=$userCode)"
}

/** Where a code stands. */
sealed interface DevicePoll {
    /** Nobody answered yet. */
    data object Waiting : DevicePoll

    /** Signed in, as the profile the approver picked. */
    data class SignedIn(val issuer: String) : DevicePoll

    /** The approver said no. */
    data object Denied : DevicePoll

    /** The code ran out: get a new one. */
    data object Expired : DevicePoll

    /**
     * The server can't be reached: polling goes on by itself in
     * [retryInMs]. Waiting comes again once it answers.
     */
    data class Unreachable(val retryInMs: Long) : DevicePoll

    /** It can't go on: an older server, or a refusal, in [text]. */
    data class Failed(val text: UiText) : DevicePoll
}

/**
 * Signing in without a browser, with the device authorization grant: the
 * TV's only way in (TVs may not open a browser), and the phone's when it
 * has none. Only viewer tokens come of it, whoever approves.
 */
interface DeviceSignIn {
    /** Asks [server] for a code to show. */
    suspend fun start(server: String): DeviceCode

    /**
     * Asks whether [code] was approved, every interval while collected
     * (slower when the server says so), until it is signed in, denied or
     * expired. Network trouble shows as Unreachable while it lasts.
     */
    fun poll(code: DeviceCode): Flow<DevicePoll>
}

/**
 * The device authorization grant (RFC 8628 §3) against a pley server:
 * POST client_id and scope to the metadata's device_authorization_endpoint
 * for a code, then ask the token endpoint with it.
 *
 * Polling waits the server's interval between questions, 5 seconds longer
 * after each slow_down (§3.5), and never asks sooner than the interval.
 * When the network fails it waits 2, 4, 8, 16, then 30 seconds. The tokens
 * it gets go through the scope guard, and replace (and end) the ones this
 * app held for that server, as a sign-in in the browser does.
 */
class DeviceGrant(
    private val http: OkHttpClient,
    private val tokensOf: (String) -> Tokens,
    private val clock: () -> Long = System::currentTimeMillis,
) : DeviceSignIn {
    override suspend fun start(server: String): DeviceCode {
        val home = Addresses.origin(server) ?: throw AuthException(NOT_AN_ADDRESS)
        Transport.checkAddress(home)
        val metadata = ServerMetadata.fetch(http, home)
        val endpoint = metadata.deviceAuthorizationEndpoint?.takeIf { metadata.signsInWithCodes }
            ?: throw AuthException(NO_CODES)
        val scope = SignIn.scopeFor(metadata, withCode = true)
        val answer = TokenEndpoint.post(http, endpoint, listOf("client_id" to CLIENT_ID, "scope" to scope))
        return parse(answer, home, metadata, scope, clock())
    }

    override fun poll(code: DeviceCode): Flow<DevicePoll> = flow {
        var interval = (code.intervalSeconds.takeIf { it > 0 } ?: DEFAULT_INTERVAL_S) * 1000L
        var failures = 0
        var troubled = false
        emit(DevicePoll.Waiting)
        while (true) {
            val wait = waitAfter(failures, interval)
            delay(minOf(wait, (code.expiresAt - clock()).coerceAtLeast(0)))
            if (clock() >= code.expiresAt) {
                emit(DevicePoll.Expired)
                return@flow
            }
            val outcome: DevicePoll? = try {
                val answer = TokenEndpoint.post(
                    http,
                    code.tokenEndpoint,
                    listOf("grant_type" to DEVICE_CODE_GRANT, "device_code" to code.deviceCode, "client_id" to CLIENT_ID),
                )
                failures = 0
                keep(code, answer)
            } catch (e: TokenException) {
                failures = 0
                when (e.oauth?.error) {
                    "authorization_pending" -> null
                    "slow_down" -> null.also { interval += SLOW_DOWN_MS }
                    "access_denied" -> DevicePoll.Denied
                    "expired_token" -> DevicePoll.Expired
                    // invalid_grant: the code is unknown, or was collected already.
                    "invalid_grant" -> DevicePoll.Failed(UiText.res(R.string.auth_code_used))
                    else -> DevicePoll.Failed(e.text)
                }
            } catch (e: AuthException) {
                // Refused by the app itself: plain http beyond the home network.
                DevicePoll.Failed(e.text)
            } catch (_: IOException) {
                failures++
                null
            }
            if (outcome != null) {
                emit(outcome)
                return@flow
            }
            if (failures > 0) {
                troubled = true
                emit(DevicePoll.Unreachable(waitAfter(failures, interval)))
            } else if (troubled) {
                troubled = false
                emit(DevicePoll.Waiting)
            }
        }
    }

    /** How long to wait after [failures] network failures in a row, asking every [interval] ms otherwise. */
    private fun waitAfter(failures: Int, interval: Long): Long =
        if (failures == 0) interval else maxOf(interval, BACKOFF_MS[minOf(failures, BACKOFF_MS.size) - 1])

    /** Keeps the tokens in [answer]; what to show. */
    private suspend fun keep(code: DeviceCode, answer: String): DevicePoll = try {
        val parsed = TokenResponse.parse(answer, code.scope)
        val fresh = parsed.stored(code.issuer, code.tokenEndpoint, code.revocationEndpoint, clock())
        keepNewSignIn(http, tokensOf(code.issuer), parsed, code.scope, fresh)
        DevicePoll.SignedIn(code.issuer)
    } catch (e: AuthException) {
        // Unreadable, or more than was asked (ended already).
        DevicePoll.Failed(e.text)
    }

    companion object {
        /** RFC 8628 §3.2: without an interval, 5 seconds. */
        const val DEFAULT_INTERVAL_S = 5

        /** What a slow_down adds to this wait and every later one (§3.5). */
        const val SLOW_DOWN_MS = 5_000L

        /** The waits after network failures in a row. */
        val BACKOFF_MS = longArrayOf(2_000, 4_000, 8_000, 16_000, 30_000)

        /** The code in [text], the answer of [server]'s device authorization endpoint (§3.2). */
        fun parse(text: String, server: String, metadata: ServerMetadata, scope: String, now: Long): DeviceCode {
            val bad = AuthException(UNREADABLE)
            val o = try {
                JSONObject(text)
            } catch (_: JSONException) {
                throw bad
            }
            val deviceCode = o.optString("device_code").ifEmpty { throw bad }
            val userCode = shown(o.optString("user_code")).ifEmpty { throw bad }
            val verificationUri = link(o.optString("verification_uri")) ?: throw bad
            val complete = if (o.isNull("verification_uri_complete")) null else link(o.optString("verification_uri_complete"))
            val expiresIn = o.optLong("expires_in", -1)
            if (expiresIn <= 0) throw bad
            val interval = if (o.isNull("interval")) DEFAULT_INTERVAL_S else o.optInt("interval", DEFAULT_INTERVAL_S)
            return DeviceCode(
                server = server,
                deviceCode = deviceCode,
                userCode = userCode,
                verificationUri = verificationUri,
                verificationUriComplete = complete,
                expiresAt = now + expiresIn * 1000,
                intervalSeconds = interval.takeIf { it > 0 } ?: DEFAULT_INTERVAL_S,
                issuer = metadata.issuer,
                tokenEndpoint = metadata.tokenEndpoint,
                revocationEndpoint = metadata.revocationEndpoint,
                scope = scope,
            )
        }

        /** A user code as people read it: 8 letters as `BCDF-GHJK`, anything else as sent. */
        fun shown(code: String): String {
            val letters = code.uppercase().filter { it.isLetterOrDigit() }
            return if (letters.length == 8) letters.take(4) + "-" + letters.drop(4) else plain(code)
        }

        /** [text] when it is a web address to show (http or https), else `null`. */
        private fun link(text: String): String? {
            val clean = plain(text)
            if (clean.isEmpty() || clean != text) return null
            val uri = try {
                URI(clean)
            } catch (_: URISyntaxException) {
                return null
            }
            val scheme = uri.scheme?.lowercase()
            return clean.takeIf { (scheme == "http" || scheme == "https") && !uri.host.isNullOrEmpty() }
        }
    }
}
