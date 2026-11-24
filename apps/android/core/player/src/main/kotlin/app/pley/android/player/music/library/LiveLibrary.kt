package app.pley.android.player.music.library

import android.net.Uri
import android.os.Bundle
import androidx.annotation.OptIn
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.MediaConstants
import app.pley.android.data.DataGraph
import app.pley.android.data.LiveTvRepository
import app.pley.android.model.LiveCategory
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveGroups
import app.pley.android.model.LiveNames
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope

/** The words and icon of Live TV's nodes, in the language the app is shown in. */
interface LiveLook {
    val liveTv: String

    /** The tab's groups: the favourites, the channels watched lately, then the provider's groups. */
    val favorites: String
    val watchedLately: String
    val groups: String

    /** A lineup without groups lists every channel under this. */
    val allChannels: String

    /** Search's group of channels. */
    val channels: String

    /** "120 channels". */
    fun channelCount(count: Int): String

    /** The tab's icon: monochrome, as an `android.resource` address. */
    val liveIcon: Uri?
}

/** Live TV as the car reads it: whether to show it, its groups, a category's channels, and one channel. */
interface LiveSource {
    /** Whether Live TV is open to this profile and has channels. */
    suspend fun shown(): Boolean

    suspend fun groups(): LiveGroups

    /** [category]'s channels matching [query], [limit] at most, in the server's order (the closest first when searched). */
    suspend fun channels(category: LiveCategory, query: String = "", limit: Int = LiveLibrary.MAX_CHANNELS): List<LiveChannel>

    suspend fun channel(id: Long): LiveChannel?
}

/** [LiveSource] over the app's Live TV repository, as the music's source waits for its data. */
class RepositoryLiveSource(private val data: DataGraph) : LiveSource {
    private val live: LiveTvRepository get() = data.liveTv

    override suspend fun shown(): Boolean = live.summary().settledIn(data).let { it.allowed && it.available }

    override suspend fun groups(): LiveGroups = live.groups().settledIn(data)

    override suspend fun channels(category: LiveCategory, query: String, limit: Int): List<LiveChannel> {
        val found = mutableListOf<LiveChannel>()
        var page = 0
        while (found.size < limit) {
            val got = live.page(category, query, page).settledIn(data)
            found += got.channels
            if (got.channels.isEmpty() || got.offset + got.channels.size >= got.total) break
            page++
        }
        // Pages read one after another can overlap as the list changes between them (a channel watched meanwhile).
        return found.distinctBy { it.id }.take(limit)
    }

    override suspend fun channel(id: Long): LiveChannel? = live.channel(id).settledIn(data)
}

/**
 * Live TV in the car (Android Auto's media screens), its channels'
 * sound played as radio ([app.pley.android.player.music.LiveRadio]): a
 * tab of its own, the fourth (cars show four at most), shown only to a
 * profile Live TV is open to. It lists the favourites, then the channels
 * watched lately, each under its heading, then the provider's groups;
 * each channel with its logo and what is on. Search finds channels too,
 * and a spoken name plays its channel. Groups and channels are named as
 * the phone's lists name them ([LiveNames]): "UK · Sport", and in it
 * "Sky Sports 1" rather than the provider's "UK| Sky Sports 1".
 */
@OptIn(markerClass = [UnstableApi::class])
class LiveLibrary(
    private val source: LiveSource,
    private val look: () -> LiveLook,
    private val art: (path: String) -> Uri?,
    /** Now, in unix seconds: what is on. */
    private val now: () -> Long = { System.currentTimeMillis() / 1000 },
) {
    /** The tab, or `null` when Live TV isn't open to the profile or has no channels. */
    suspend fun tab(): MediaItem? {
        if (!runCatching { source.shown() }.getOrDefault(false)) return null
        val l = look()
        return node(MusicId.LiveTv, l.liveTv, icon = l.liveIcon, mediaType = MediaMetadata.MEDIA_TYPE_FOLDER_TV_CHANNELS)
    }

    /** The node [id] names, without its children; `null` for one that isn't Live TV's. */
    suspend fun item(id: MusicId): MediaItem? = when (id) {
        MusicId.LiveTv -> tab()
        is MusicId.LiveList -> list(id.category, title(id.category), subtitle = null)
        is MusicId.Channel -> source.channel(id.id)?.let { channel(it, id.from) }
        else -> null
    }

    /** [id]'s children; `null` for one that isn't Live TV's. */
    suspend fun children(id: MusicId): List<MediaItem>? = when (id) {
        MusicId.LiveTv -> tabChildren()
        is MusicId.LiveList -> source.channels(id.category).map { channel(it, id.category) }
        else -> null
    }

    /** The channels matching [query], under their heading. */
    suspend fun search(query: String): List<MediaItem> {
        if (!runCatching { source.shown() }.getOrDefault(false)) return emptyList()
        val heading = look().channels
        return source.channels(LiveCategory.All, query, SEARCH_LIMIT).map { channel(it, null).grouped(heading) }
    }

    /**
     * What playing [id] plays: the channels of the list it was picked
     * from, and where it is in them, so Next and Previous go through the
     * list; a channel found alone plays alone. `null` when it is gone.
     */
    suspend fun lineup(id: MusicId.Channel): Pair<List<LiveChannel>, Int>? {
        val from = id.from
        if (from != null) {
            val list = source.channels(from)
            val at = list.indexOfFirst { it.id == id.id }
            if (at >= 0) return list to at
        }
        return source.channel(id.id)?.let { listOf(it) to 0 }
    }

    /**
     * The channel a spoken request names exactly ("play BBC One on Pley"), or `null`: by its name, else by its name
     * without the provider's region ("BBC One" for "UK| BBC One"), which is what a list shows and a driver reads.
     */
    suspend fun named(query: String): LiveChannel? {
        val q = query.trim()
        if (q.isEmpty() || !runCatching { source.shown() }.getOrDefault(false)) return null
        val found = source.channels(LiveCategory.All, q, SEARCH_LIMIT)
        return found.firstOrNull { it.name.trim().equals(q, ignoreCase = true) }
            ?: found.firstOrNull { LiveNames.splitRegion(it.name)?.second.equals(q, ignoreCase = true) }
    }

    /** The closest channel to [query], for a spoken request that matched no music. */
    suspend fun closest(query: String): LiveChannel? {
        if (query.isBlank() || !runCatching { source.shown() }.getOrDefault(false)) return null
        return source.channels(LiveCategory.All, query.trim(), 1).firstOrNull()
    }

    /**
     * [channel] as the car shows it, playing or in a list: its name (in a group, without the region the group says),
     * what is on, and its logo.
     */
    fun channel(channel: LiveChannel, from: LiveCategory?): MediaItem {
        val on = channel.onAt(now())?.title
        val name = LiveNames.channelName(channel, from)
        val metadata = MediaMetadata.Builder()
            .setTitle(name)
            .setDisplayTitle(name)
            .setSubtitle(on)
            .setArtist(on)
            .setArtworkUri(channel.logo?.let { art(logoPath(channel.id)) })
            .setIsBrowsable(false)
            .setIsPlayable(true)
            .setMediaType(MediaMetadata.MEDIA_TYPE_RADIO_STATION)
            .build()
        return MediaItem.Builder().setMediaId(MusicId.Channel(channel.id, from).text).setMediaMetadata(metadata).build()
    }

    /** The favourites, the channels watched lately (not again), and the groups, or every channel without them. */
    private suspend fun tabChildren(): List<MediaItem> = coroutineScope {
        val l = look()
        val groups = async { source.groups() }
        val favorites = async { source.channels(LiveCategory.Favorites, limit = TAB_EACH) }
        val recent = async { source.channels(LiveCategory.Recent, limit = TAB_EACH) }
        val shown = groups.await()
        val favored = if (shown.favorites > 0) favorites.await() else emptyList()
        val lately = if (shown.recent > 0) recent.await().filter { c -> favored.none { it.id == c.id } } else emptyList()
        val lists = if (shown.groups.size > 1) {
            shown.groups.map { list(LiveCategory.Group(it.name), LiveNames.groupName(it, it.name), l.channelCount(it.count)).grouped(l.groups) }
        } else {
            listOf(list(LiveCategory.All, l.allChannels, l.channelCount(shown.total)).grouped(l.groups))
        }
        favored.map { channel(it, LiveCategory.Favorites).grouped(l.favorites) } +
            lately.map { channel(it, LiveCategory.Recent).grouped(l.watchedLately) } +
            lists
    }

    private fun title(category: LiveCategory): String {
        val l = look()
        return when (category) {
            LiveCategory.All -> l.allChannels
            LiveCategory.Favorites -> l.favorites
            LiveCategory.Recent -> l.watchedLately
            // As the group is listed ([LiveNames.groupName]), from its name alone, as the phone's player titles it.
            is LiveCategory.Group -> LiveNames.splitRegion(category.name)?.let { (region, label) -> "$region · $label" } ?: category.name
        }
    }

    private fun list(category: LiveCategory, title: String, subtitle: String?): MediaItem =
        node(MusicId.LiveList(category), title, subtitle = subtitle, mediaType = MediaMetadata.MEDIA_TYPE_FOLDER_TV_CHANNELS)

    private fun node(id: MusicId, title: String, subtitle: String? = null, icon: Uri? = null, mediaType: Int): MediaItem {
        // Channels list in rows: their logos are wide, and their names and what's on are what one reads.
        val extras = Bundle().apply {
            putInt(MediaConstants.EXTRAS_KEY_CONTENT_STYLE_BROWSABLE, MediaConstants.EXTRAS_VALUE_CONTENT_STYLE_LIST_ITEM)
            putInt(MediaConstants.EXTRAS_KEY_CONTENT_STYLE_PLAYABLE, MediaConstants.EXTRAS_VALUE_CONTENT_STYLE_LIST_ITEM)
        }
        val metadata = MediaMetadata.Builder()
            .setTitle(title)
            .setDisplayTitle(title)
            .setSubtitle(subtitle)
            .setArtworkUri(icon)
            .setIsBrowsable(true)
            .setIsPlayable(false)
            .setMediaType(mediaType)
            .setExtras(extras)
            .build()
        return MediaItem.Builder().setMediaId(id.text).setMediaMetadata(metadata).build()
    }

    private fun MediaItem.grouped(group: String): MediaItem {
        val extras = Bundle(mediaMetadata.extras ?: Bundle()).apply { putString(MediaConstants.EXTRAS_KEY_CONTENT_STYLE_GROUP_TITLE, group) }
        return buildUpon().setMediaMetadata(mediaMetadata.buildUpon().setExtras(extras).build()).build()
    }

    companion object {
        /** The most channels a list holds: cars don't page, and a provider can list tens of thousands. */
        const val MAX_CHANNELS = 500

        /** The most favourites and channels watched lately the tab lists. */
        private const val TAB_EACH = 50

        /** The most channels a search shows. */
        private const val SEARCH_LIMIT = 15

        /** A channel's logo on the server, as [MusicArt] serves it. */
        fun logoPath(channelId: Long): String = "/api/live/channels/$channelId/logo"
    }
}
