package app.pley.android.model.admin

import app.pley.android.model.LenientEnumSerializer
import app.pley.android.model.LivePlan
import app.pley.android.model.WireEnum
import kotlinx.serialization.Serializable

/*
 * Live TV's lineup as administrators see it (docs/API.md, "Live TV"), and
 * the connections in use. The app's own Live TV types (LiveChannel,
 * LiveGroup…) are what people watch; these are what administrators
 * manage. Live TV's settings and sources hold providers' passwords and
 * stay in the web app.
 */

/** A channel as administrators see it: as the source has it, and as set here. */
@Serializable
data class LineupChannel(
    val id: Long,
    val sourceId: Long = 0,
    val sourceName: String = "",
    val name: String = "",
    val number: String? = null,
    val group: String = "",
    val logo: String? = null,
    /** The guide channel it uses. */
    val guideId: String = "",
    val hidden: Boolean = false,
    val hasGuide: Boolean = false,
    val sourceNumber: String? = null,
    val sourceGuideId: String = "",
    /** The guide's channel of its name, when the source names none. */
    val matchedGuideId: String? = null,
    val customName: String? = null,
    val customNumber: String? = null,
    val customGuideId: String? = null,
    /** It didn't play when last tried, or it is a provider's divider row. */
    val offline: Boolean = false,
)

/** GET /api/live/lineup/page: [channels] from [offset] of [total], of which [hidden] are hidden. */
@Serializable
data class LineupPage(
    val total: Int = 0,
    val hidden: Int = 0,
    val offset: Int = 0,
    val channels: List<LineupChannel> = emptyList(),
)

/** A group as administrators see it (GET /api/live/lineup/groups): hidden and offline channels counted. */
@Serializable
data class LineupGroup(
    val name: String,
    /** The region its name begins with ("UK" of "UK| Sport"). */
    val region: String? = null,
    /** The name without its region ("Sport"). */
    val label: String = name,
    val count: Int = 0,
    val hidden: Int = 0,
    val offline: Int = 0,
)

/** Which of the lineup's channels are listed or changed. */
@Serializable(with = LineupState.Serializer::class)
enum class LineupState(override val wire: String) : WireEnum {
    All("all"),
    Shown("shown"),
    Hidden("hidden"),
    NoGuide("no-guide"),
    Offline("offline"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<LineupState>("LineupState", entries, Unknown)
}

/** The lineup's channels to list or change: of a group, in a state, matching [q]. */
@Serializable
data class LineupFilter(
    val group: String? = null,
    val state: LineupState = LineupState.All,
    /** A name, or a number's beginning. */
    val q: String? = null,
)

/**
 * PATCH /api/live/lineup: [ids], every channel of [groups] and every one
 * [filter] keeps. What is `null` is left out and stays; `""` gives back
 * the source's own name, number or guide channel.
 */
@Serializable
data class LineupPatch(
    val ids: List<Long> = emptyList(),
    val groups: List<String>? = null,
    val filter: LineupFilter? = null,
    val hidden: Boolean? = null,
    val name: String? = null,
    val number: String? = null,
    val guideId: String? = null,
)

/** Someone watching a channel a tuner pulls. */
@Serializable
data class TunerViewer(
    val sessionId: String = "",
    val username: String = "",
)

/** A connection in use (GET /api/live/tuners): the channel pulled and who watches it. */
@Serializable
data class TunerStatus(
    val channelId: Long,
    val channelName: String = "",
    val sourceName: String = "",
    val startedAt: Long = 0,
    /** The provider is sending. */
    val connected: Boolean = false,
    val error: String? = null,
    /** What the provider sends. */
    val kbps: Long = 0,
    val viewers: List<TunerViewer> = emptyList(),
    /** Nobody watches: it goes soon. */
    val idle: Boolean = false,
    /** What the transcoder makes of it, for HLS viewers. */
    val plan: LivePlan? = null,
)
