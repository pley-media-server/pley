package app.pley.android.fixtures

import app.pley.android.model.CastAction
import app.pley.android.model.CastControlRequest
import app.pley.android.model.CastDevice
import app.pley.android.model.CastIdentity
import app.pley.android.model.CastPlatform
import app.pley.android.model.CastPlayRequest
import app.pley.android.model.CastPlayState
import app.pley.android.model.CastState
import app.pley.android.model.ItemKind
import app.pley.android.model.PleyJson
import app.pley.android.model.ServerEvent
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.longOrNull
import mockwebserver3.MockResponse

/**
 * The devices that may be cast to, by profile, each listed while one of
 * its event streams is open (apps/server/src/cast.rs). Behind the state's lock.
 */
internal class CastList {
    class Device(var name: String, val platform: CastPlatform) {
        var streams = 0
        var state: CastState? = null
        var saidAtMs = 0L
    }

    private val devices = LinkedHashMap<Pair<Long, String>, Device>()

    fun device(profileId: Long, id: String): Device? = devices[profileId to id]

    /** Whether [profileId] may list [id] now: it is listed already, or has room. */
    fun hasRoom(profileId: Long, id: String): Boolean =
        (profileId to id) in devices || devices.keys.count { it.first == profileId } < MAX_EACH

    fun connect(profileId: Long, cast: CastIdentity) {
        val device = devices.getOrPut(profileId to cast.id) { Device(cast.name, cast.platform) }
        device.name = cast.name
        device.streams++
    }

    fun disconnect(profileId: Long, id: String) {
        val device = devices[profileId to id] ?: return
        if (--device.streams <= 0) devices.remove(profileId to id)
    }

    /** GET /api/cast/devices: [profileId]'s, by name. */
    fun list(profileId: Long, nowMs: Long): List<CastDevice> = devices.filterKeys { it.first == profileId }
        .map { (key, d) -> CastDevice(key.second, d.name, d.platform, d.state?.copy(ageMs = (nowMs - d.saidAtMs).coerceAtLeast(0))) }
        .sortedBy { it.name.lowercase() }

    companion object {
        const val MAX_EACH = 32
        const val POSITION_MAX = 7L * 24 * 3_600_000

        /** 8 to 64 letters, digits, `-` and `_`. */
        fun validId(id: String): Boolean = id.length in 8..64 && id.all { (it.isLetterOrDigit() && it.code < 128) || it == '-' || it == '_' }
    }
}

/**
 * Casting (docs/API.md, "Casting to another device"): GET /api/cast/devices,
 * and POST play and control, which reach the device's own stream as
 * `cast_play` and `cast_control`, and PUT state, which the profile's
 * streams hear as `cast_state`. Devices are listed by the event stream
 * (EventHub.open).
 */
internal class CastRoutes(private val s: State) {
    fun routes(): List<Route> = listOf(
        route("GET", "/api/cast/devices", PLAY) { _, c, _ ->
            Reply.json(ListSerializer(CastDevice.serializer()), s.casts.list(c.profile.id, s.nowMs()))
        },
        route("POST", "/api/cast/devices/*/play", PLAY) { call, c, v -> play(call, c, v[0]) },
        route("POST", "/api/cast/devices/*/control", PLAY) { call, c, v -> control(call, c, v[0]) },
        route("PUT", "/api/cast/devices/*/state", PLAY) { call, c, v -> report(call, c, v[0]) },
    )

    private fun play(call: Call, caller: Caller, id: String): MockResponse {
        s.casts.device(caller.profile.id, id) ?: return notConnected()
        val body = call.body(CastPlayRequest.serializer()) ?: return Reply.badRequest("the body isn't a title to play")
        val item = s.catalog.item(body.itemId) ?: return Reply.notFound("item ${body.itemId} not found")
        if (item.kind != ItemKind.Movie && item.kind != ItemKind.Episode && item.kind != ItemKind.Extra) {
            return Reply.badRequest("only movies, episodes and extras are cast")
        }
        if (body.fileId != null && item.files.none { it.id == body.fileId }) return Reply.notFound("file not found")
        if (body.positionMs != null && body.positionMs !in 0..CastList.POSITION_MAX) return badPosition()
        s.events.emitToDevice(caller.profile.id, id, ServerEvent.CastPlay(id, item.id, body.fileId, body.positionMs))
        return Reply.noContent()
    }

    private fun control(call: Call, caller: Caller, id: String): MockResponse {
        s.casts.device(caller.profile.id, id) ?: return notConnected()
        val body = call.body(CastControlRequest.serializer())?.takeIf { it.action != CastAction.Unknown }
            ?: return Reply.badRequest("the body isn't a command")
        val position = when {
            body.action != CastAction.Seek -> null
            body.positionMs == null -> return Reply.badRequest("seek needs position_ms")
            body.positionMs !in 0..CastList.POSITION_MAX -> return badPosition()
            else -> body.positionMs
        }
        s.events.emitToDevice(caller.profile.id, id, ServerEvent.CastControl(id, body.action, position))
        return Reply.noContent()
    }

    private fun report(call: Call, caller: Caller, id: String): MockResponse {
        val device = s.casts.device(caller.profile.id, id) ?: return notConnected()
        val body = runCatching { PleyJson.parseToJsonElement(call.text) as? JsonObject }.getOrNull() ?: return Reply.badRequest("the body isn't JSON")
        val itemId = (body["item_id"] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.longOrNull
        val state = if (itemId == null) {
            null
        } else {
            val item = s.views.summary(itemId, caller.profile) ?: return Reply.notFound("item $itemId not found")
            val position = (body["position_ms"] as? JsonPrimitive)?.longOrNull ?: 0
            val duration = (body["duration_ms"] as? JsonPrimitive)?.longOrNull
            if (position !in 0..CastList.POSITION_MAX || (duration != null && duration !in 0..CastList.POSITION_MAX)) return badPosition()
            val playState = when ((body["state"] as? JsonPrimitive)?.content) {
                null, "playing" -> CastPlayState.Playing
                "paused" -> CastPlayState.Paused
                "buffering" -> CastPlayState.Buffering
                else -> return Reply.badRequest("unknown state")
            }
            CastState(item, position, duration, playState, ageMs = 0)
        }
        device.state = state
        device.saidAtMs = s.nowMs()
        s.events.emitTo(caller.profile.id, ServerEvent.CastStateChanged(id, state))
        return Reply.noContent()
    }

    private fun notConnected() = Reply.notFound("that device isn't connected")

    private fun badPosition() = Reply.badRequest("a position is 0 to a week in milliseconds")
}
