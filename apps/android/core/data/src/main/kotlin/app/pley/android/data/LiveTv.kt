package app.pley.android.data

import app.pley.android.common.NotBuiltYet
import app.pley.android.model.Control
import app.pley.android.model.LiveCategory
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveChannelPage
import app.pley.android.model.LiveGroups
import app.pley.android.model.LiveGuide
import app.pley.android.model.LivePlan
import app.pley.android.model.LiveSummary
import app.pley.android.model.LiveWatching
import app.pley.android.model.WatchRequest
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch

/**
 * Live TV (docs/API.md, "Live TV"): whether to show it, its groups, the
 * channels of a category a page at a time, the guide of the channels in
 * view, favourites, and watching a channel. A provider lists tens of
 * thousands of channels: nothing here asks for them all.
 */
interface LiveTvRepository {
    /** Whether Live TV is open to this profile and has channels. */
    fun summary(): Flow<Loadable<LiveSummary>>

    /** The groups in the lineup's order, and how many channels are in all, marked and watched lately. */
    fun groups(): Flow<Loadable<LiveGroups>>

    /** Page [page] ([PAGE] channels) of [category]'s channels matching [query]. */
    fun page(category: LiveCategory, query: String, page: Int): Flow<Loadable<LiveChannelPage>>

    /** The page of [category] that holds [channelId]: where a list opens again, and the channels either side of it. */
    fun around(category: LiveCategory, channelId: Long): Flow<Loadable<LiveChannelPage>>

    /** One channel, with what's on now and next. */
    fun channel(channelId: Long): Flow<Loadable<LiveChannel>>

    /** The guide of the channels [ids] from [from] for [GUIDE_CHUNK_S] (a multiple of it, unix seconds). */
    fun guide(from: Long, ids: List<Long>): Flow<Loadable<LiveGuide>>

    /** The channel numbered [number] exactly, of every channel; `null` when none is. */
    suspend fun numbered(number: String): LiveChannel?

    /** Asks again for what's on and the groups' counts: a programme ended, or the page came back. */
    fun refresh()

    /** A favourite channel or not: at once on screen, put back if the server refuses. */
    suspend fun setFavorite(channelId: Long, favorite: Boolean)

    /**
     * Starts watching [channelId] on this device, [clientId] ending what
     * it watched before. [LiveWatching.url] is absolute. Throws the
     * server's refusal (`409`: every connection of its source in use,
     * [app.pley.android.network.ApiException.connections] saying what holds
     * each). [takeOver]: the channel whose connection to take, one the
     * `409` said may be. [controls]: what the player obeys from the
     * remote (channel up and down).
     */
    suspend fun watch(channelId: Long, clientId: String, takeOver: Long? = null, controls: List<Control> = emptyList()): LiveWatching

    /** What [sessionId]'s channel is made into, waiting a few seconds for it; `null` when not known by then. */
    suspend fun plan(sessionId: String): LivePlan?

    /**
     * The address of [channelId]'s MPEG-TS as the provider sends it
     * (`stream.ts`), absolute, for [clientId]: reading it ends that
     * device's HLS viewer, as watching another channel does. It says what
     * kind of device reads it, as watching does.
     */
    suspend fun streamUrl(channelId: Long, clientId: String): String

    /** Stops watching [sessionId], in the background: it outlives the screen that asks. */
    fun leave(sessionId: String)

    companion object {
        /** Channels asked for at once, as the web asks. */
        const val PAGE = 100

        /** The guide is fetched in chunks of six hours, a page of channels at a time. */
        const val GUIDE_CHUNK_S = 6 * 3600L

        /** No Live TV: tests' graphs, which have none. */
        val None: LiveTvRepository = object : LiveTvRepository {
            override fun summary(): Flow<Loadable<LiveSummary>> = flowOf(Loadable.Ready(LiveSummary(), fresh = true))

            override fun groups(): Flow<Loadable<LiveGroups>> = flowOf(Loadable.Ready(LiveGroups(), fresh = true))

            override fun page(category: LiveCategory, query: String, page: Int): Flow<Loadable<LiveChannelPage>> =
                flowOf(Loadable.Ready(LiveChannelPage(), fresh = true))

            override fun around(category: LiveCategory, channelId: Long): Flow<Loadable<LiveChannelPage>> =
                flowOf(Loadable.Ready(LiveChannelPage(), fresh = true))

            override fun channel(channelId: Long): Flow<Loadable<LiveChannel>> = flowOf(Loadable.Failed(NotBuiltYet("Live TV")))

            override fun guide(from: Long, ids: List<Long>): Flow<Loadable<LiveGuide>> =
                flowOf(Loadable.Ready(LiveGuide(from, from + GUIDE_CHUNK_S), fresh = true))

            override suspend fun numbered(number: String): LiveChannel? = null

            override fun refresh() = Unit

            override suspend fun setFavorite(channelId: Long, favorite: Boolean) = throw NotBuiltYet("Live TV")

            override suspend fun watch(channelId: Long, clientId: String, takeOver: Long?, controls: List<Control>): LiveWatching =
                throw NotBuiltYet("Live TV")

            override suspend fun plan(sessionId: String): LivePlan? = null

            override suspend fun streamUrl(channelId: Long, clientId: String): String = throw NotBuiltYet("Live TV")

            override fun leave(sessionId: String) = Unit
        }
    }
}

/**
 * Live TV through the cache. The summary, the groups and each category's
 * first page are kept as snapshots, so Live TV opens at once; other pages,
 * channels and the guide are held in memory with item pages (the least
 * recently used going first). A favourite changes every page, channel and
 * count that shows it.
 */
internal class CachedLiveTv(private val cache: ResourceCache) : LiveTvRepository {
    private val summary = Resource(SnapshotKeys.LIVE, LiveSummary.serializer()) { api, _ -> api.liveSummary() }

    private val groups = Resource(SnapshotKeys.LIVE_GROUPS, LiveGroups.serializer()) { api, _ -> api.liveGroups() }

    override fun summary(): Flow<Loadable<LiveSummary>> = cache.observe(summary)

    override fun groups(): Flow<Loadable<LiveGroups>> = cache.observe(groups)

    override fun page(category: LiveCategory, query: String, page: Int): Flow<Loadable<LiveChannelPage>> {
        val q = query.trim()
        // A category's first page, unsearched, is what opens: it is kept on disk.
        val kept = page == 0 && q.isEmpty()
        return cache.observe(
            Resource(
                SnapshotKeys.livePage(category.key, q, page),
                if (kept) LiveChannelPage.serializer() else null,
                detail = !kept,
            ) { api, _ -> api.liveBrowse(category, q, offset = page * LiveTvRepository.PAGE, limit = LiveTvRepository.PAGE) },
        )
    }

    override fun around(category: LiveCategory, channelId: Long): Flow<Loadable<LiveChannelPage>> = cache.observe(
        Resource<LiveChannelPage>(SnapshotKeys.liveAround(category.key, channelId), serializer = null, detail = true) { api, _ ->
            api.liveBrowse(category, limit = LiveTvRepository.PAGE, around = channelId)
        },
    )

    override fun channel(channelId: Long): Flow<Loadable<LiveChannel>> = cache.observe(
        Resource<LiveChannel>(SnapshotKeys.liveChannel(channelId), serializer = null, detail = true) { api, _ -> api.liveChannel(channelId) },
    )

    override fun guide(from: Long, ids: List<Long>): Flow<Loadable<LiveGuide>> = cache.observe(
        Resource<LiveGuide>(SnapshotKeys.liveGuide(from, ids), serializer = null, detail = true) { api, _ ->
            api.liveGuide(from, from + LiveTvRepository.GUIDE_CHUNK_S, ids)
        },
    )

    override suspend fun numbered(number: String): LiveChannel? =
        cache.api(cache.profile()).liveBrowse(LiveCategory.All, number, limit = LiveTvRepository.PAGE).channels.firstOrNull { it.number == number }

    override fun refresh() = cache.revalidate { key, _ ->
        key == SnapshotKeys.LIVE || key == SnapshotKeys.LIVE_GROUPS || key.startsWith("live:page:") ||
            key.startsWith("live:around:") || key.startsWith("live:channel:")
    }

    override suspend fun setFavorite(channelId: Long, favorite: Boolean) {
        fun mark(c: LiveChannel) = if (c.id == channelId && c.favorite != favorite) c.copy(favorite = favorite) else c
        val patched = cache.patchAll { value ->
            when (value) {
                is LiveChannelPage -> value.channels.map(::mark).takeIf { it != value.channels }?.let { value.copy(channels = it) }
                is LiveChannel -> mark(value).takeIf { it !== value }
                is LiveGroups -> value.copy(favorites = (value.favorites + if (favorite) 1 else -1).coerceAtLeast(0))
                else -> null
            }
        }
        undoing(patched) { cache.api(cache.profile()).setFavoriteChannel(channelId, favorite) }
        // Favourites' own pages and counts come from the server.
        cache.revalidate { key, _ -> key == SnapshotKeys.LIVE_GROUPS || key.startsWith("live:page:favorites:") || key.startsWith("live:around:favorites:") }
    }

    override suspend fun watch(channelId: Long, clientId: String, takeOver: Long?, controls: List<Control>): LiveWatching {
        val api = cache.api(cache.profile())
        val watching = api.watchChannel(channelId, WatchRequest(clientId, takeOver, controls.takeIf { it.isNotEmpty() }))
        // Recently watched has it first now: asked again when next shown.
        cache.markStale { key, _ ->
            key == SnapshotKeys.LIVE || key == SnapshotKeys.LIVE_GROUPS || key.startsWith("live:page:recent:") || key.startsWith("live:around:recent:")
        }
        return watching.copy(url = api.url(watching.url))
    }

    override suspend fun plan(sessionId: String): LivePlan? = cache.api(cache.profile()).livePlan(sessionId)

    override suspend fun streamUrl(channelId: Long, clientId: String): String =
        cache.api(cache.profile()).liveStreamUrl(channelId, clientId)

    override fun leave(sessionId: String) {
        cache.app.launch {
            try {
                cache.api(cache.profile()).leaveChannel(sessionId)
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                // Out of reach: the server lets the session go by itself after 45 s.
            }
        }
    }
}
