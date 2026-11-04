package app.pley.android.model

import kotlinx.serialization.Serializable

/**
 * GET /api/live: whether to show Live TV. [allowed]: it is open to this
 * profile; [available]: and it has channels. [sources] is for
 * administrators (0 for the others); [recent] the channels watched last,
 * newest first.
 */
@Serializable
data class LiveSummary(
    val available: Boolean = false,
    val allowed: Boolean = false,
    val channels: Int = 0,
    val sources: Int = 0,
    val recent: List<Long> = emptyList(),
)

/**
 * A programme of the guide (docs/API.md, `LiveProgram`). Times are unix
 * seconds; [image] is an address on the server, when the guide has a
 * picture.
 */
@Serializable
data class LiveProgram(
    val channelId: Long,
    val start: Long,
    val stop: Long,
    val title: String = "",
    val subtitle: String? = null,
    val description: String? = null,
    val categories: List<String> = emptyList(),
    val image: String? = null,
    val season: Int? = null,
    val episode: Int? = null,
    val isNew: Boolean = false,
    /** Shown as it happens (a match, the news). */
    val isLive: Boolean = false,
    val rating: String? = null,
) {
    /** On at [now] (unix seconds). */
    fun airingAt(now: Long): Boolean = start <= now && stop > now

    /** How far it is at [now], from 0 to 1. */
    fun progressAt(now: Long): Float =
        ((now - start).toFloat() / (stop - start).coerceAtLeast(1)).coerceIn(0f, 1f)

    /** "S2 E14", or "E14", when the guide numbers the episode. */
    val episodeLabel: String?
        get() = when {
            season != null && episode != null -> "S$season E$episode"
            episode != null -> "E$episode"
            else -> null
        }
}

/**
 * A channel of the lineup, with what's on [now] and [next]
 * (docs/API.md, `LiveChannel`). [logo] is its logo's address on the server.
 */
@Serializable
data class LiveChannel(
    val id: Long,
    val sourceId: Long = 0,
    val name: String,
    val number: String? = null,
    val group: String = "",
    val logo: String? = null,
    val favorite: Boolean = false,
    /** The guide has programmes for it. */
    val hasGuide: Boolean = false,
    val now: LiveProgram? = null,
    val next: LiveProgram? = null,
) {
    /** What is on at [now] (unix seconds); `null` once [LiveChannel.now] ended, before the list is asked again. */
    fun onAt(now: Long): LiveProgram? = this.now?.takeIf { it.airingAt(now) } ?: next?.takeIf { it.airingAt(now) }

    /** What comes after [onAt]: [next], or what was listed as now when it hasn't begun. */
    fun nextAt(now: Long): LiveProgram? {
        val on = onAt(now) ?: return listOfNotNull(this.now, next).firstOrNull { it.start > now }
        return listOfNotNull(this.now, next).firstOrNull { it.start >= on.stop }
    }
}

/**
 * A group of channels as the provider names it (docs/API.md,
 * `LiveGroup`): [region] is the "UK" of "UK| Sport", [label] the rest.
 */
@Serializable
data class LiveGroup(
    val name: String,
    val region: String? = null,
    val label: String = name,
    val count: Int = 0,
)

/** GET /api/live/groups: the groups in the lineup's order, and how many channels in all, marked and watched lately. */
@Serializable
data class LiveGroups(
    val total: Int = 0,
    val favorites: Int = 0,
    val recent: Int = 0,
    val groups: List<LiveGroup> = emptyList(),
)

/** GET /api/live/browse: [channels] from [offset] of a list of [total]. */
@Serializable
data class LiveChannelPage(
    val total: Int = 0,
    val offset: Int = 0,
    val channels: List<LiveChannel> = emptyList(),
)

/**
 * Which channels a list has (`category` of GET /api/live/browse): every
 * one, the favourites, those watched lately, or one group's.
 */
sealed interface LiveCategory {
    /** How it is kept and passed on: "all", "favorites", "recent" or "group:UK| Sport". */
    val key: String

    /** `category` on the wire. */
    val wire: String

    data object All : LiveCategory {
        override val key: String get() = "all"
        override val wire: String get() = "all"
    }

    data object Favorites : LiveCategory {
        override val key: String get() = "favorites"
        override val wire: String get() = "favorites"
    }

    data object Recent : LiveCategory {
        override val key: String get() = "recent"
        override val wire: String get() = "recent"
    }

    data class Group(val name: String) : LiveCategory {
        override val key: String get() = "group:$name"
        override val wire: String get() = "group"
    }

    companion object {
        fun parse(key: String?): LiveCategory = when {
            key == "favorites" -> Favorites
            key == "recent" -> Recent
            key != null && key.startsWith("group:") -> Group(key.removePrefix("group:"))
            else -> All
        }
    }
}

/** GET /api/live/guide: the programmes between [from] and [to] (unix seconds), by channel. */
@Serializable
data class LiveGuide(
    val from: Long,
    val to: Long,
    val programs: Map<Long, List<LiveProgram>> = emptyMap(),
)

/**
 * The body of POST /api/live/channels/{id}/watch: [clientId] ends what
 * this device watched before, so changing channels never holds two of the
 * provider's connections. [takeOver], every connection being in use: the
 * channel whose connection to take (one the `409` said may be,
 * [BusyConnection.canTakeOver]); those watching it are told who took it.
 */
@Serializable
data class WatchRequest(
    val clientId: String? = null,
    val takeOver: Long? = null,
    /** The remote's commands this player obeys, as a play request's. */
    val controls: List<Control>? = null,
    /** What remotes call this device, as a play request's. */
    val deviceName: String? = null,
)

/**
 * POST /api/live/channels/{id}/watch's answer: [url] is the channel's
 * live HLS playlist, read as [sessionId] until DELETE
 * /api/live/sessions/{session} (or 45 s without a request).
 */
/**
 * GET /api/live/sessions/{session}/plan: what the channel a session watches
 * carries and what the transcoder makes of it ([video]: `copy` or the
 * encoder; [audio]: `copy`, `aac` or `none`). Sent as it is, a player
 * that reads MPEG-TS plays it from `stream.ts`, sooner than its first HLS
 * segments ([direct]).
 */
@Serializable
data class LivePlan(
    val videoCodec: String? = null,
    val width: Int? = null,
    val height: Int? = null,
    val frameRate: Double? = null,
    val interlaced: Boolean = false,
    val audioCodec: String? = null,
    val audioChannels: Int? = null,
    val video: String = "",
    val audio: String = "",
) {
    /** The channel is sent as it is (or without picture or sound): `stream.ts` plays it straight away, as the web's player does. */
    val direct: Boolean get() = video in SENT_AS_IS && audio in SENT_AS_IS
}

/** A [LivePlan]'s video or audio as the provider sends it, or none of it. */
private val SENT_AS_IS = setOf("copy", "none")

@Serializable
data class LiveWatching(
    val sessionId: String,
    val url: String,
    val kind: String = "hls",
    val channel: LiveChannel? = null,
)
