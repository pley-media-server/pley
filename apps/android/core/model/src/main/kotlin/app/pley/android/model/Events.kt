package app.pley.android.model

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.descriptors.buildClassSerialDescriptor
import kotlinx.serialization.descriptors.element
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonEncoder
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject

/** Where a library scan is (apps/server/src/events.rs, `ScanPhase`). */
@Serializable(with = ScanPhase.Serializer::class)
enum class ScanPhase(override val wire: String) : WireEnum {
    Queued("queued"),

    /** Looking through the folders. */
    Walking("walking"),

    /** Reading the new and changed files. */
    Probing("probing"),

    /** Fetching titles, artwork and people. */
    Metadata("metadata"),
    Done("done"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<ScanPhase>("ScanPhase", ScanPhase.entries, ScanPhase.Unknown)
}

/**
 * One message of GET /api/events, the server's stream of Server-Sent
 * Events: each `data:` is a JSON object whose `type` says which
 * (apps/server/src/events.rs, `ServerEvent`). Events about libraries the
 * profile can't open never come. A `type` this app doesn't know is
 * [Unknown], so a newer server can add events. [parse] reads one.
 */
@Serializable(with = ServerEvent.Serializer::class)
sealed interface ServerEvent {
    /** The `type` on the wire. */
    val type: String

    /** The library the event is about, if it is about one. */
    val libraryId: Long? get() = null

    @Serializable
    data class ScanStarted(override val libraryId: Long) : ServerEvent {
        override val type: String get() = TYPE

        companion object {
            const val TYPE = "scan_started"
        }
    }

    @Serializable
    data class ScanProgress(
        override val libraryId: Long,
        val phase: ScanPhase = ScanPhase.Unknown,
        val processed: Long = 0,
        val total: Long = 0,
    ) : ServerEvent {
        override val type: String get() = TYPE

        companion object {
            const val TYPE = "scan_progress"
        }
    }

    @Serializable
    data class ScanFinished(
        override val libraryId: Long,
        val added: Long = 0,
        val updated: Long = 0,
        val removed: Long = 0,
    ) : ServerEvent {
        override val type: String get() = TYPE

        companion object {
            const val TYPE = "scan_finished"
        }
    }

    /** Items or their details changed (a scan, markers found, something watched): read them again. */
    @Serializable
    data class LibraryChanged(override val libraryId: Long) : ServerEvent {
        override val type: String get() = TYPE

        companion object {
            const val TYPE = "library_changed"
        }
    }

    /** A library was made, changed or deleted, or their order changed. */
    data object LibrariesChanged : ServerEvent {
        const val TYPE = "libraries_changed"
        override val type: String get() = TYPE
    }

    /** Playback sessions started, stopped or moved on. */
    data object SessionsChanged : ServerEvent {
        const val TYPE = "sessions_changed"
        override val type: String get() = TYPE
    }

    /** The profile's My List changed on another device. */
    data object WatchlistChanged : ServerEvent {
        const val TYPE = "watchlist_changed"
        override val type: String get() = TYPE
    }

    /**
     * The profile's watch state of an item changed, here or on another
     * device: watched or not, a resume point started, moved or cleared, in
     * or out of Continue Watching. [itemIds] is the item, then its parent
     * and grandparent (a season and its show), whose counts follow it;
     * [children]: everything under the first one changed too (a show or a
     * season marked as a whole).
     */
    @Serializable
    data class UserStateChanged(
        val itemIds: List<Long> = emptyList(),
        val children: Boolean = false,
    ) : ServerEvent {
        override val type: String get() = TYPE

        companion object {
            const val TYPE = "user_state_changed"
        }
    }

    /**
     * A playback session was stopped by the server (DELETE /api/sessions/{id}):
     * by an administrator when [byAdmin], else by the profile from another
     * device. [message] says why, when given.
     */
    @Serializable
    data class SessionStopped(
        val sessionId: String,
        val message: String? = null,
        val byAdmin: Boolean = false,
    ) : ServerEvent {
        override val type: String get() = TYPE

        companion object {
            const val TYPE = "session_stopped"
        }
    }

    /**
     * The profile's remote, on another device, sent the player of
     * [sessionId] a command (POST /api/sessions/{id}/command): [positionMs]
     * for a seek, [offsetMs] for a skip, `null` otherwise. Only the player
     * playing that session acts on it.
     */
    @Serializable
    data class PlayerCommand(
        val sessionId: String,
        val command: Control = Control.Unknown,
        val positionMs: Long? = null,
        val offsetMs: Long? = null,
    ) : ServerEvent {
        override val type: String get() = TYPE

        companion object {
            const val TYPE = "player_command"
        }
    }

    /** Live TV's sources, channels or guide changed: the channels and the guide are asked for again. */
    data object LiveChannelsChanged : ServerEvent {
        const val TYPE = "live_channels_changed"
        override val type: String get() = TYPE
    }

    /** A live channel was tuned or let go, or its viewers changed: the connections in use changed. */
    data object LiveTunersChanged : ServerEvent {
        const val TYPE = "live_tuners_changed"
        override val type: String get() = TYPE
    }

    /**
     * Someone took the connection of [channelId], watched by this profile,
     * to watch another channel: every player of it stops and shows
     * [message] (who took it only when both are in the home).
     */
    @Serializable
    data class LiveTakenOver(
        val sessionId: String,
        val channelId: Long,
        val message: String,
    ) : ServerEvent {
        override val type: String get() = TYPE

        companion object {
            const val TYPE = "live_taken_over"
        }
    }

    /**
     * A request changed (its stage too): for its maker, its followers and
     * managers. All three `null` when many changed (another device looked
     * at its requests, an account was deleted).
     */
    @Serializable
    data class RequestsChanged(
        val requestId: Long? = null,
        val kind: TitleKind? = null,
        val tmdbId: Long? = null,
    ) : ServerEvent {
        override val type: String get() = TYPE

        companion object {
            const val TYPE = "requests_changed"
        }
    }

    /**
     * A request is ready to watch, for its maker and followers: [itemId] is
     * where this account can open it. A show's request also says so when
     * some of its seasons are ready ([seasons], not [complete]).
     */
    @Serializable
    data class RequestAvailable(
        val requestId: Long,
        val title: String = "",
        val kind: TitleKind = TitleKind.Unknown,
        val tmdbId: Long = 0,
        val itemId: Long? = null,
        val posterUrl: String? = null,
        val seasons: List<Int>? = null,
        val complete: Boolean = true,
    ) : ServerEvent {
        override val type: String get() = TYPE

        companion object {
            const val TYPE = "request_available"
        }
    }

    /** A request was approved or declined: for its maker, and when declined its followers ([reason] `null` for them). */
    @Serializable
    data class RequestDecided(
        val requestId: Long,
        val title: String = "",
        val kind: TitleKind = TitleKind.Unknown,
        val tmdbId: Long = 0,
        val status: RequestStatus = RequestStatus.Unknown,
        val reason: String? = null,
    ) : ServerEvent {
        override val type: String get() = TYPE

        companion object {
            const val TYPE = "request_decided"
        }
    }

    /** Someone else asked for something: for managers. */
    @Serializable
    data class RequestCreated(
        val requestId: Long,
        val title: String = "",
        val kind: TitleKind = TitleKind.Unknown,
        val tmdbId: Long = 0,
        val seasons: List<Int>? = null,
        val requestedBy: RequestUser? = null,
        val status: RequestStatus = RequestStatus.Unknown,
    ) : ServerEvent {
        override val type: String get() = TYPE

        companion object {
            const val TYPE = "request_created"
        }
    }

    /** A request couldn't be sent and waits for a manager: for managers. */
    @Serializable
    data class RequestFailed(
        val requestId: Long,
        val title: String = "",
        val kind: TitleKind = TitleKind.Unknown,
        val reason: String? = null,
        val retryAt: Long? = null,
    ) : ServerEvent {
        override val type: String get() = TYPE

        companion object {
            const val TYPE = "request_failed"
        }
    }

    /** Requests were turned on or off, their limits or TMDB's key changed, or what this account may do with them. */
    data object RequestsSettingsChanged : ServerEvent {
        const val TYPE = "requests_settings_changed"
        override val type: String get() = TYPE
    }

    /** An event this app doesn't know: skipped, but not an error. */
    data class Unknown(override val type: String) : ServerEvent

    /**
     * Reads and writes events by their `type`; the other fields are read
     * by each event's own serializer. JSON only.
     */
    object Serializer : KSerializer<ServerEvent> {
        override val descriptor: SerialDescriptor =
            buildClassSerialDescriptor("app.pley.android.model.ServerEvent") { element<String>("type") }

        override fun deserialize(decoder: Decoder): ServerEvent {
            val input = decoder as? JsonDecoder ?: throw SerializationException("server events are read from JSON")
            val tree = input.decodeJsonElement() as? JsonObject
                ?: throw SerializationException("a server event is a JSON object")
            val type = (tree["type"] as? JsonPrimitive)?.takeIf { it.isString }?.content.orEmpty()
            val fields = JsonObject(tree - "type")
            val json = input.json
            return when (type) {
                ScanStarted.TYPE -> json.decodeFromJsonElement(ScanStarted.serializer(), fields)
                ScanProgress.TYPE -> json.decodeFromJsonElement(ScanProgress.serializer(), fields)
                ScanFinished.TYPE -> json.decodeFromJsonElement(ScanFinished.serializer(), fields)
                LibraryChanged.TYPE -> json.decodeFromJsonElement(LibraryChanged.serializer(), fields)
                LibrariesChanged.TYPE -> LibrariesChanged
                SessionsChanged.TYPE -> SessionsChanged
                SessionStopped.TYPE -> json.decodeFromJsonElement(SessionStopped.serializer(), fields)
                PlayerCommand.TYPE -> json.decodeFromJsonElement(PlayerCommand.serializer(), fields)
                WatchlistChanged.TYPE -> WatchlistChanged
                UserStateChanged.TYPE -> json.decodeFromJsonElement(UserStateChanged.serializer(), fields)
                LiveChannelsChanged.TYPE -> LiveChannelsChanged
                LiveTunersChanged.TYPE -> LiveTunersChanged
                LiveTakenOver.TYPE -> json.decodeFromJsonElement(LiveTakenOver.serializer(), fields)
                RequestsChanged.TYPE -> json.decodeFromJsonElement(RequestsChanged.serializer(), fields)
                RequestAvailable.TYPE -> json.decodeFromJsonElement(RequestAvailable.serializer(), fields)
                RequestDecided.TYPE -> json.decodeFromJsonElement(RequestDecided.serializer(), fields)
                RequestCreated.TYPE -> json.decodeFromJsonElement(RequestCreated.serializer(), fields)
                RequestFailed.TYPE -> json.decodeFromJsonElement(RequestFailed.serializer(), fields)
                RequestsSettingsChanged.TYPE -> RequestsSettingsChanged
                else -> Unknown(type)
            }
        }

        override fun serialize(encoder: Encoder, value: ServerEvent) {
            val output = encoder as? JsonEncoder ?: throw SerializationException("server events are written as JSON")
            val json = output.json
            val fields: Map<String, JsonElement> = when (value) {
                is ScanStarted -> json.encodeToJsonElement(ScanStarted.serializer(), value).jsonObject
                is ScanProgress -> json.encodeToJsonElement(ScanProgress.serializer(), value).jsonObject
                is ScanFinished -> json.encodeToJsonElement(ScanFinished.serializer(), value).jsonObject
                is LibraryChanged -> json.encodeToJsonElement(LibraryChanged.serializer(), value).jsonObject
                is SessionStopped -> json.encodeToJsonElement(SessionStopped.serializer(), value).jsonObject
                is PlayerCommand -> json.encodeToJsonElement(PlayerCommand.serializer(), value).jsonObject
                is LiveTakenOver -> json.encodeToJsonElement(LiveTakenOver.serializer(), value).jsonObject
                is UserStateChanged -> json.encodeToJsonElement(UserStateChanged.serializer(), value).jsonObject
                is RequestsChanged -> json.encodeToJsonElement(RequestsChanged.serializer(), value).jsonObject
                is RequestAvailable -> json.encodeToJsonElement(RequestAvailable.serializer(), value).jsonObject
                is RequestDecided -> json.encodeToJsonElement(RequestDecided.serializer(), value).jsonObject
                is RequestCreated -> json.encodeToJsonElement(RequestCreated.serializer(), value).jsonObject
                is RequestFailed -> json.encodeToJsonElement(RequestFailed.serializer(), value).jsonObject
                LibrariesChanged, SessionsChanged, WatchlistChanged, LiveChannelsChanged, LiveTunersChanged, RequestsSettingsChanged, is Unknown -> emptyMap()
            }
            output.encodeJsonElement(JsonObject(mapOf<String, JsonElement>("type" to JsonPrimitive(value.type)) + fields))
        }
    }

    companion object {
        /**
         * One event from the `data:` of a Server-Sent Event. Throws
         * [SerializationException] for text that isn't a JSON object, or a
         * known event missing a field it needs.
         */
        fun parse(text: String): ServerEvent = PleyJson.decodeFromString(Serializer, text)
    }
}
