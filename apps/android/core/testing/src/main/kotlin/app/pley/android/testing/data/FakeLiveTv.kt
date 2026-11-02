package app.pley.android.testing.data

import app.pley.android.data.LiveTvRepository
import app.pley.android.data.Loadable
import app.pley.android.model.LiveCategory
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveChannelPage
import app.pley.android.model.LiveGroups
import app.pley.android.model.LiveGuide
import app.pley.android.model.LivePlan
import app.pley.android.model.LiveSummary
import app.pley.android.model.LiveWatching
import app.pley.android.testing.Fixtures
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map

/**
 * Live TV as the data side keeps it, for tests: a lineup ([lineup]: the
 * fixtures' three channels unless the test sets another), paged as the
 * server pages it ([LiveTvRepository.PAGE]), guide chunks the test sets
 * ([guides]), and what was sent, in order ([sent]: "watch 1 android-1",
 * "leave live-1", "favorite 2 false", "guide 0 [1, 2]", "refresh"…).
 * [watchFails] makes watching refuse, [failingPages] pages fail.
 */
class FakeLiveTv : LiveTvRepository {
    private val pageSize = LiveTvRepository.PAGE

    val summary = MutableStateFlow<Loadable<LiveSummary>>(Loadable.Ready(Fixtures.decode("live/summary.json"), fresh = true))
    val groups = MutableStateFlow<Loadable<LiveGroups>>(Loadable.Ready(Fixtures.decode("live/groups.json"), fresh = true))
    val lineup = MutableStateFlow<List<LiveChannel>>(Fixtures.decode("live/channels.json"))

    /** The channels watched lately, newest first. */
    val recent = MutableStateFlow(listOf(2L, 1L))
    val guides = HashMap<Long, MutableStateFlow<Loadable<LiveGuide>>>()
    val sent = mutableListOf<String>()

    /** Watching fails with this until set back to `null`. */
    var watchFails: Exception? = null

    /** Setting a favourite fails with this until set back to `null`. */
    var favoriteFails: Exception? = null

    /** What every session's channel is made into: `null` (not known in time, the HLS is played) unless the test sets one. */
    var plan: LivePlan? = null

    /** Makes asking for a plan refuse: a `410` for a session the server ended. */
    var planFails: Exception? = null

    /** The sessions whose plan was asked for, in order. */
    val plansAsked = mutableListOf<String>()

    /** Pages, by number, that fail when asked for, until taken out. */
    val failingPages = HashMap<Int, Exception>()

    /** The pages asked for, by number, in order: each time a page's flow is collected, as the cache asks for one it lacks. */
    val pagesAsked = mutableListOf<Int>()

    fun guideChunk(from: Long): MutableStateFlow<Loadable<LiveGuide>> = guides.getOrPut(from) { MutableStateFlow(Loadable.Loading) }

    private fun list(category: LiveCategory, query: String): Flow<List<LiveChannel>> = combine(lineup, recent) { all, recentIds ->
        val inCategory = when (category) {
            LiveCategory.All -> all
            LiveCategory.Favorites -> all.filter { it.favorite }
            LiveCategory.Recent -> recentIds.mapNotNull { id -> all.firstOrNull { it.id == id } }
            is LiveCategory.Group -> all.filter { it.group == category.name }
        }
        val q = query.trim().lowercase()
        if (q.isEmpty()) inCategory else inCategory.filter { it.name.lowercase().contains(q) || it.number?.startsWith(q) == true }
    }

    override fun summary(): Flow<Loadable<LiveSummary>> = summary

    override fun groups(): Flow<Loadable<LiveGroups>> = groups

    override fun page(category: LiveCategory, query: String, page: Int): Flow<Loadable<LiveChannelPage>> = flow {
        pagesAsked += page
        val fails = failingPages[page]
        if (fails != null) {
            emit(Loadable.Failed(fails))
        } else {
            emitAll(
                list(category, query).map { all ->
                    Loadable.Ready(LiveChannelPage(all.size, page * pageSize, all.drop(page * pageSize).take(pageSize)), fresh = true)
                },
            )
        }
    }

    override fun around(category: LiveCategory, channelId: Long): Flow<Loadable<LiveChannelPage>> =
        list(category, "").map { all ->
            val start = all.indexOfFirst { it.id == channelId }.coerceAtLeast(0) / pageSize * pageSize
            Loadable.Ready(LiveChannelPage(all.size, start, all.drop(start).take(pageSize)), fresh = true)
        }

    override fun channel(channelId: Long): Flow<Loadable<LiveChannel>> =
        lineup.map { all -> all.firstOrNull { it.id == channelId }?.let { Loadable.Ready(it, fresh = true) } ?: Loadable.Loading }

    override fun guide(from: Long, ids: List<Long>): Flow<Loadable<LiveGuide>> {
        sent += "guide $from $ids"
        return guideChunk(from)
    }

    override suspend fun numbered(number: String): LiveChannel? = lineup.value.firstOrNull { it.number == number }

    override fun refresh() {
        sent += "refresh"
    }

    override suspend fun setFavorite(channelId: Long, favorite: Boolean) {
        sent += "favorite $channelId $favorite"
        favoriteFails?.let { throw it }
        lineup.value = lineup.value.map { if (it.id == channelId) it.copy(favorite = favorite) else it }
    }

    override suspend fun watch(channelId: Long, clientId: String, takeOver: Long?): LiveWatching {
        sent += "watch $channelId $clientId" + (takeOver?.let { " taking $it" } ?: "")
        watchFails?.let { throw it }
        return LiveWatching("live-$channelId", "http://127.0.0.1:7539/api/live/sessions/live-$channelId/index.m3u8")
    }

    override suspend fun plan(sessionId: String): LivePlan? {
        plansAsked += sessionId
        planFails?.let { throw it }
        return plan
    }

    override suspend fun streamUrl(channelId: Long, clientId: String): String =
        "http://127.0.0.1:7539/api/live/channels/$channelId/stream.ts?client_id=$clientId"

    override fun leave(sessionId: String) {
        sent += "leave $sessionId"
    }
}
