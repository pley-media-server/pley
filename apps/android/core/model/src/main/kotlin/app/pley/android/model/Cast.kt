package app.pley.android.model

import kotlinx.serialization.Serializable

/** What a device to cast to is, for the icon beside its name (apps/server/src/cast.rs, `Platform`). */
@Serializable(with = CastPlatform.Serializer::class)
enum class CastPlatform(override val wire: String) : WireEnum {
    Web("web"),
    Desktop("desktop"),
    Android("android"),
    AndroidTv("android_tv"),
    Kodi("kodi"),
    Roku("roku"),
    Samsung("samsung"),
    Lg("lg"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<CastPlatform>("CastPlatform", CastPlatform.entries, CastPlatform.Unknown)
}

/** Whether a cast device's player is moving (apps/server/src/cast.rs, `PlayState`). */
@Serializable(with = CastPlayState.Serializer::class)
enum class CastPlayState(override val wire: String) : WireEnum {
    Playing("playing"),
    Paused("paused"),

    /** Loading, or waiting for more of the stream. */
    Buffering("buffering"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<CastPlayState>("CastPlayState", CastPlayState.entries, CastPlayState.Unknown)
}

/** A command for a cast device's player. */
@Serializable(with = CastAction.Serializer::class)
enum class CastAction(override val wire: String) : WireEnum {
    Pause("pause"),
    Resume("resume"),

    /** To [CastControlRequest.positionMs]. */
    Seek("seek"),

    /** Leaves the player, as Back would. */
    Stop("stop"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<CastAction>("CastAction", CastAction.entries, CastAction.Unknown)
}

/**
 * What a cast device plays, as it last said. [ageMs] is how long before
 * the answer it said so: a playing position has moved on since.
 */
@Serializable
data class CastState(
    val item: ItemSummary,
    val positionMs: Long = 0,
    val durationMs: Long? = null,
    val state: CastPlayState = CastPlayState.Playing,
    val ageMs: Long = 0,
)

/** GET /api/cast/devices: one of the profile's devices that may be cast to. */
@Serializable
data class CastDevice(
    val id: String,
    val name: String,
    val platform: CastPlatform = CastPlatform.Unknown,
    /** What it plays; `null`: nothing. */
    val state: CastState? = null,
)

/** The body of POST /api/cast/devices/{id}/play: [positionMs] `null` starts where the profile left off. */
@Serializable
data class CastPlayRequest(
    val itemId: Long,
    val fileId: Long? = null,
    val positionMs: Long? = null,
)

/** The body of POST /api/cast/devices/{id}/control: [positionMs] is where a seek goes. */
@Serializable
data class CastControlRequest(
    val action: CastAction,
    val positionMs: Long? = null,
)

/**
 * The body of PUT /api/cast/devices/{id}/state: what this device plays.
 * [itemId] `null` plays nothing; the server reads it left out (PleyJson
 * writes no nulls) as `null`.
 */
@Serializable
data class CastReport(
    val itemId: Long? = null,
    val positionMs: Long = 0,
    val durationMs: Long? = null,
    val state: CastPlayState = CastPlayState.Playing,
)

/**
 * How a device that may be cast to is listed while its event stream is
 * open (GET /api/events?cast_device=&cast_name=&cast_platform=): [id] it
 * made once and keeps, [name] what people call it.
 */
data class CastIdentity(
    val id: String,
    val name: String,
    val platform: CastPlatform,
) {
    companion object {
        /** The longest name the server takes, in characters. */
        const val NAME_MAX = 64

        /** [name] as the server takes it: trimmed, 1 to [NAME_MAX] characters, never cut halfway through one. */
        fun nameOf(name: String, fallback: String): String {
            val trimmed = name.trim().ifEmpty { fallback }
            if (trimmed.codePointCount(0, trimmed.length) <= NAME_MAX) return trimmed
            return trimmed.substring(0, trimmed.offsetByCodePoints(0, NAME_MAX)).trim()
        }
    }
}
