package app.pley.android.model

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull

/**
 * GET /api/server, which answers without signing in: the server's name and
 * version, whether it still has to be set up in the browser, whether it
 * requires two-step sign-in, and its language (apps/server/src/api/auth.rs,
 * `server_status`).
 */
@Serializable
data class ServerInfo(
    val name: String,
    val version: String,
    val setupRequired: Boolean = false,
    /** Setting up takes the code from the server's log. */
    val setupCodeRequired: Boolean = false,
    /**
     * Every account with a password must use two-step sign-in: signing in
     * on Pley's page sets it up for whoever hasn't yet. Servers before it
     * leave it out.
     */
    @Serializable(with = LenientFlagSerializer::class)
    val twoStepRequired: Boolean = false,
    /**
     * The server's language for accounts that haven't chosen one ("en",
     * "fr", "es"); `null` when each device's own is used, and from servers
     * before it.
     */
    val language: String? = null,
)

/** GET /api/health: `{ "status": "ok" }` while the server runs. */
@Serializable
data class Health(
    val status: String,
)

/**
 * A flag that is on only when the server sends `true`: anything else (a
 * string, a number, an object) reads as off instead of failing the whole
 * answer, so an odd value never makes a server look like something else.
 */
internal object LenientFlagSerializer : KSerializer<Boolean> {
    override val descriptor: SerialDescriptor =
        PrimitiveSerialDescriptor("app.pley.android.model.LenientFlag", PrimitiveKind.BOOLEAN)

    override fun deserialize(decoder: Decoder): Boolean {
        val json = decoder as? JsonDecoder ?: return decoder.decodeBoolean()
        val value = json.decodeJsonElement()
        return value is JsonPrimitive && !value.isString && value.booleanOrNull == true
    }

    override fun serialize(encoder: Encoder, value: Boolean) = encoder.encodeBoolean(value)
}
