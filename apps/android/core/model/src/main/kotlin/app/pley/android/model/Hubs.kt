package app.pley.android.model

import kotlinx.serialization.Serializable

/**
 * A row of the home page or of a library's front page
 * (apps/server/src/api/libraries.rs, `Hub`). Empty rows are left out.
 */
@Serializable
data class Hub(
    /**
     * "continue_watching" on the home page, then "my_list"; "recently_added:<library id>"
     * there and on a library's page, with "continue_watching:<id>",
     * "recently_released:<id>" and "top_rated:<id>".
     */
    val key: String,
    val title: String,
    /** `null` for the home page's Continue Watching and My List. */
    val libraryId: Long? = null,
    val items: List<ItemSummary> = emptyList(),
    /**
     * The collections the row shows: one, or on Home several of one title in
     * different libraries; none for Continue Watching and My List.
     */
    val collectionIds: List<Long> = emptyList(),
) {
    /** Home's or a library's Continue Watching. */
    val continueWatching: Boolean get() = HubKeys.isContinueWatching(key)
}

/** The keys of the hubs the app treats as their own kind. */
object HubKeys {
    /** Home's Continue Watching; a library's is "continue_watching:<id>". */
    const val CONTINUE_WATCHING = "continue_watching"

    /** Home's My List, after Continue Watching. */
    const val MY_LIST = "my_list"

    fun isContinueWatching(key: String): Boolean = key == CONTINUE_WATCHING || key.startsWith("$CONTINUE_WATCHING:")
}

/**
 * `{ hubs }`: GET /api/hubs/home (Continue Watching, My List, then Recently
 * Added of each library in the libraries' order) and GET /api/libraries/{id}/hubs.
 */
@Serializable
data class Hubs(
    val hubs: List<Hub> = emptyList(),
)
