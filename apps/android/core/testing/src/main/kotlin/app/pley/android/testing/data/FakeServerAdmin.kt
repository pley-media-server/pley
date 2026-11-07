package app.pley.android.testing.data

import app.pley.android.data.AdminAccess
import app.pley.android.data.ServerAdmin
import app.pley.android.model.PleyJson
import app.pley.android.network.AdminApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.serialization.DeserializationStrategy
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

/**
 * A server to manage, for the pages of managing the server's tests: what
 * each `METHOD /path` answers ([answer]), and every call made ([calls]), with
 * its query and body. A route without an answer fails as a server's 404 would, with
 * [failWith] standing in for anything else.
 */
class FakeServerAdmin(access: AdminAccess = AdminAccess.Manages) : ServerAdmin {
    /** One call: `"PATCH /api/users/2"`, the JSON it sent, and its query (what was sent of it, in order). */
    data class Call(val route: String, val body: JsonElement?, val query: List<Pair<String, String>> = emptyList())

    val accessState = MutableStateFlow(access)
    override val access: Flow<AdminAccess> get() = accessState

    private val answers = mutableMapOf<String, (JsonElement?) -> JsonElement?>()
    val calls = mutableListOf<Call>()

    /** What makes every call fail from now on (`null`: none), as when the server is out of reach. */
    var failWith: Throwable? = null

    /** `METHOD /path` (query left out) answers [json]; `null` for `204`. */
    fun answer(route: String, json: String?) {
        answers[route] = { json?.let(PleyJson::parseToJsonElement) }
    }

    /** `METHOD /path` answers what [reply] makes of the body sent. */
    fun answer(route: String, reply: (JsonElement?) -> JsonElement?) {
        answers[route] = reply
    }

    /** `METHOD /path` fails with [error]. */
    fun fail(route: String, error: Throwable) {
        answers[route] = { throw error }
    }

    /** The calls made to [route], in order. */
    fun callsTo(route: String): List<Call> = calls.filter { it.route == route }

    override suspend fun api(): AdminApi = api

    private val api = object : AdminApi {
        override suspend fun <T> get(answer: DeserializationStrategy<T>, path: List<Any>, query: List<Pair<String, String?>>): T =
            PleyJson.decodeFromJsonElement(answer, reply("GET", path, null, query) ?: JsonObject(emptyMap()))

        override suspend fun <T> send(method: String, answer: DeserializationStrategy<T>, path: List<Any>, body: JsonElement?): T =
            PleyJson.decodeFromJsonElement(answer, reply(method, path, body) ?: JsonObject(emptyMap()))

        override suspend fun call(method: String, path: List<Any>, body: JsonElement?) {
            reply(method, path, body)
        }
    }

    private fun reply(method: String, path: List<Any>, body: JsonElement?, query: List<Pair<String, String?>> = emptyList()): JsonElement? {
        val route = "$method /" + path.joinToString("/")
        calls += Call(route, body, query.mapNotNull { (key, value) -> value?.let { key to it } })
        failWith?.let { throw it }
        val reply = answers[route] ?: throw NoSuchElementException("no answer for $route")
        return reply(body)
    }
}
