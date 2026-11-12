package app.pley.android.network

import kotlinx.serialization.DeserializationStrategy
import kotlinx.serialization.json.JsonElement

/**
 * Managing the server, as Settings in the web app do (docs/API.md): the
 * administrators' routes, which answer Pley's own apps allowed `server` on
 * an administrator's account as they answer the account's own sign-in.
 * Each part of Settings sends and reads its own types (core/model's
 * `admin` package) through these three calls.
 *
 * Paths are segments, each encoded (`listOf("api", "users", 2)`). Bodies
 * are JSON as the server reads it: snake_case names, and a `null` written
 * out where the server takes it to mean "remove" (a PIN, an age limit),
 * which [app.pley.android.model.PleyJson] leaves out of what it encodes,
 * so such a body is built with `buildJsonObject`. Failures are the app's
 * own ([ApiException], [OfflineException]…), as for [PleyApi].
 */
interface AdminApi {
    /** GET [path] with [query] (in its order, `null`s left out), read as [answer]. */
    suspend fun <T> get(answer: DeserializationStrategy<T>, path: List<Any>, query: List<Pair<String, String?>> = emptyList()): T

    /** [method] [path] with [body] (`null`: none), read as [answer]. */
    suspend fun <T> send(method: String, answer: DeserializationStrategy<T>, path: List<Any>, body: JsonElement? = null): T

    /** [method] [path] with [body] (`null`: none), when the answer says nothing (`204`). */
    suspend fun call(method: String, path: List<Any>, body: JsonElement? = null)

    /** Where there is no server to manage (tests, fakes): every call fails. */
    object Unavailable : AdminApi {
        override suspend fun <T> get(answer: DeserializationStrategy<T>, path: List<Any>, query: List<Pair<String, String?>>): T =
            throw UnsupportedOperationException("no server to manage")

        override suspend fun <T> send(method: String, answer: DeserializationStrategy<T>, path: List<Any>, body: JsonElement?): T =
            throw UnsupportedOperationException("no server to manage")

        override suspend fun call(method: String, path: List<Any>, body: JsonElement?) =
            throw UnsupportedOperationException("no server to manage")
    }
}
