package app.pley.android.testing.network

import app.pley.android.testing.Fixtures
import java.net.InetAddress
import java.util.concurrent.TimeUnit
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import mockwebserver3.RecordedRequest

/**
 * Answers as pley gives them, for MockWebServer: JSON, refusals with
 * their `WWW-Authenticate`, event streams. Bodies can come from the JSON
 * fixtures (core/testing/src/main/resources/fixtures).
 */
object MockPley {
    /** A JSON answer. */
    fun json(body: String, code: Int = 200): MockResponse = MockResponse.Builder()
        .code(code)
        .setHeader("Content-Type", "application/json")
        .body(body)
        .build()

    /** The fixture at [path] (`"items/movie_detail.json"`) as the answer. */
    fun fixture(path: String, code: Int = 200): MockResponse = json(Fixtures.text(path), code)

    /** A success without a body, as pley answers most changes. */
    fun noContent(): MockResponse = MockResponse.Builder().code(204).build()

    /**
     * A refusal as apps/server/src/error.rs writes it: `{ code, message }`,
     * with [challenge] as `WWW-Authenticate` (`Bearer error="invalid_token"`).
     */
    fun error(status: Int, code: String, message: String, challenge: String? = null): MockResponse = MockResponse.Builder()
        .code(status)
        .setHeader("Content-Type", "application/json")
        .apply { if (challenge != null) setHeader("WWW-Authenticate", challenge) }
        .body("""{"code":"$code","message":"$message"}""")
        .build()

    /** A 401 for an app's token that doesn't work any more. */
    fun invalidToken(): MockResponse = error(
        401,
        "unauthorized",
        "this token doesn't work any more: refresh it, or sign in again",
        challenge = """Bearer error="invalid_token"""",
    )

    /** A 403 for a route the app's scopes don't open; [scope] is the one that would. */
    fun insufficientScope(scope: String?): MockResponse = error(
        403,
        "insufficient_scope",
        if (scope != null) "this app wasn't allowed to do that: sign in to it again to allow it" else "apps can't do that: use Pley's web app",
        challenge = if (scope != null) """Bearer error="insufficient_scope", scope="$scope"""" else """Bearer error="insufficient_scope"""",
    )

    /** axum's own refusal of a body or query it couldn't read: plain text. */
    fun plainText(status: Int, text: String): MockResponse = MockResponse.Builder()
        .code(status)
        .setHeader("Content-Type", "text/plain; charset=utf-8")
        .body(text)
        .build()

    /**
     * An event stream that sends [events] (each a JSON object, the `data`
     * of one event), a keep-alive comment between them as axum's
     * KeepAlive does, then ends, as the server ends a stream whose token
     * expired.
     */
    fun events(vararg events: String): MockResponse = MockResponse.Builder()
        .setHeader("Content-Type", "text/event-stream")
        .setHeader("Cache-Control", "no-cache")
        .body(events.joinToString(separator = "") { ":\n\ndata: $it\n\n" })
        .build()

    /** [response] held back for [millis] before its headers. */
    fun delayed(response: MockResponse, millis: Long): MockResponse =
        response.newBuilder().headersDelay(millis, TimeUnit.MILLISECONDS).build()

    /** A MockWebServer on 127.0.0.1, started: plain http to it is allowed as to a home address. */
    fun server(): MockWebServer = MockWebServer().apply { start(InetAddress.getByName("127.0.0.1"), 0) }

    /**
     * [server]'s origin, the issuer its tokens are for: `http://127.0.0.1:<port>`.
     * By address: a hosts file may give 127.0.0.1 a name of its own, which
     * MockWebServer.url would use.
     */
    fun issuerOf(server: MockWebServer): String = "http://127.0.0.1:${server.port}"

    /** [path] (with its query) on [server], by address. */
    fun url(server: MockWebServer, path: String): String = issuerOf(server) + path
}

/**
 * The next request the server got, in the order they came. One that never
 * comes fails the test: waited for without end, as
 * [MockWebServer.takeRequest] waits, it would hold up every test after it.
 * The sign-in tests have the same wait for the MockWebServer class they use
 * (`asked` in `:core:auth`'s TestKit.kt): the two change together.
 */
fun MockWebServer.asked(): RecordedRequest =
    takeRequest(5, TimeUnit.SECONDS) ?: throw AssertionError("the server got no further request within 5 s ($requestCount in all)")

/** The request's body as text. */
val RecordedRequest.bodyText: String get() = body?.utf8().orEmpty()

/** The request's path and query as sent: `/api/libraries/1/items?sort=title&offset=0`. */
val RecordedRequest.pathAndQuery: String get() = url.encodedPath + (url.encodedQuery?.let { "?$it" } ?: "")
