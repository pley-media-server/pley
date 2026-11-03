package app.pley.android.fixtures

import app.pley.android.model.CollectionDetail
import app.pley.android.model.CollectionSort
import app.pley.android.model.CollectionSummary
import app.pley.android.model.DynamicRange
import app.pley.android.model.HistoryEntry
import app.pley.android.model.HistoryUser
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaSummary
import app.pley.android.model.Me
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.PlaylistEntry
import app.pley.android.model.PlaylistKind
import app.pley.android.model.ResolutionFilter
import app.pley.android.model.StreamKind
import app.pley.android.model.SubtitleMode
import app.pley.android.model.UserState

/**
 * The catalogue as a profile sees it: its own positions, watched marks,
 * ratings and track choices in every item, playlist and collection.
 */
internal class Views(private val catalog: Catalog) {
    /** [item]'s state for [profile]; shows, seasons, artists and albums from their episodes or tracks. */
    fun state(item: ItemDetail, profile: Profile): UserState {
        if (item.kind.playable) return profile.state(item.id)
        val leaves = catalog.leaves(item.id).map(profile::state)
        return UserState(
            watched = leaves.isNotEmpty() && leaves.all { it.watched },
            playCount = leaves.minOfOrNull { it.playCount } ?: 0,
            lastPlayedAt = leaves.mapNotNull { it.lastPlayedAt }.maxOrNull(),
            rating = profile.state(item.id).rating,
        )
    }

    /**
     * [item] as listings give it. In library listings ([listing]) the
     * backdrop and still are left out, and summaries but episodes', and
     * the best file's numbers are in, whatever the sort.
     */
    fun summary(item: ItemDetail, profile: Profile, listing: Boolean = false): ItemSummary {
        val state = state(item, profile)
        return item.toSummary().copy(
            userState = state,
            viewedLeafCount = viewed(item, profile),
            backdropUrl = if (listing) null else item.backdropUrl,
            thumbUrl = if (listing) null else item.thumbUrl,
            summary = if (listing && item.kind != ItemKind.Episode) null else item.summary,
            media = if (listing) mediaSummary(item) else null,
        )
    }

    fun summary(id: Long, profile: Profile): ItemSummary? = catalog.item(id)?.let { summary(it, profile) }

    /** GET /api/items/{id}: with the profile's state, track choices and whether it is in My List. */
    fun detail(item: ItemDetail, profile: Profile): ItemDetail = item.copy(
        userState = state(item, profile),
        viewedLeafCount = viewed(item, profile),
        files = item.files.map { file -> withChoices(item, file, profile) },
        extras = item.extras.map { extra -> extra.copy(userState = profile.state(extra.id)) },
        inWatchlist = listed(profile).any { it.id == item.id },
    )

    /**
     * [profile]'s My List, newest added first. A movie leaves it once
     * watched after it was added (by playing it to the end or by hand).
     */
    fun listed(profile: Profile): List<ItemDetail> {
        val gone = mutableListOf<Long>()
        for (entry in profile.watchlist.values) {
            val item = catalog.item(entry.itemId)
            if (item == null) {
                gone += entry.itemId
                continue
            }
            if (item.kind != ItemKind.Movie) continue
            val state = profile.state(item.id)
            when {
                !state.watched -> entry.watchedThen = false
                !entry.watchedThen || (state.lastPlayedAt ?: 0) > entry.addedAt -> gone += entry.itemId
            }
        }
        if (gone.isNotEmpty()) {
            gone.forEach(profile.watchlist::remove)
            profile.version++
        }
        return profile.watchlist.values.sortedByDescending { it.addedAt }.mapNotNull { catalog.item(it.itemId) }
    }

    /**
     * Whether [item] is out of Continue Watching for [profile]: taken out
     * (an episode with its show), and not played since.
     */
    fun hiddenFromContinue(item: ItemDetail, profile: Profile): Boolean {
        val owner = if (item.kind == ItemKind.Episode) item.grandparentId ?: item.id else item.id
        val hiddenAt = profile.hiddenFromContinue[owner] ?: return false
        val played = if (owner == item.id) {
            profile.state(item.id).lastPlayedAt
        } else {
            catalog.leaves(owner).mapNotNull { profile.state(it).lastPlayedAt }.maxOrNull()
        }
        return (played ?: 0) <= hiddenAt
    }

    private fun viewed(item: ItemDetail, profile: Profile): Int? =
        if (item.kind.playable || item.leafCount == null) null else catalog.leaves(item.id).count { profile.state(it).watched }

    /** A movie's best file; a show's episodes (an artist's tracks) on average, with its best episode's dynamic range. */
    fun mediaSummary(item: ItemDetail): MediaSummary? {
        if (!item.kind.playable) {
            val files = catalog.leaves(item.id).mapNotNull { catalog.item(it)?.files?.firstOrNull() }
            if (files.isEmpty()) return null
            return MediaSummary(
                bitrate = files.mapNotNull { it.bitrate }.average().toLong(),
                size = files.sumOf { it.size },
                width = files.mapNotNull { it.width }.maxOrNull(),
                height = files.mapNotNull { it.height }.maxOrNull(),
                dynamicRange = bestRange(files),
            )
        }
        val best = item.files.maxByOrNull { it.bitrate ?: 0L } ?: return null
        return MediaSummary(best.bitrate, best.size, best.width, best.height, best.videoCodec, rangeOf(best))
    }

    /**
     * What a library's resolution filter takes [item] for, as the server
     * does: its best file's (a show's best episode's), on the lines of the
     * posters' "4K"; `null` without a video file.
     */
    fun resolutionOf(item: ItemDetail): ResolutionFilter? {
        val files = filesOf(item)
        val width = files.mapNotNull { it.width }.maxOrNull() ?: 0
        val height = files.mapNotNull { it.height }.maxOrNull() ?: 0
        return when {
            width >= 3800 || height >= 2100 -> ResolutionFilter.Uhd
            width >= 1800 || height >= 1000 -> ResolutionFilter.FullHd
            width >= 1200 || height >= 700 -> ResolutionFilter.Hd
            width > 0 || height > 0 -> ResolutionFilter.Sd
            else -> null
        }
    }

    /** What a library's dynamic range filter takes [item] for: its best file's (a show's best episode's). */
    fun dynamicRangeOf(item: ItemDetail): DynamicRange? = bestRange(filesOf(item))

    /** A title's video files: its own, or its episodes'. */
    private fun filesOf(item: ItemDetail): List<MediaFile> =
        if (item.kind.playable) item.files else catalog.leaves(item.id).flatMap { catalog.item(it)?.files.orEmpty() }

    private fun rangeOf(file: MediaFile): DynamicRange? = file.streams.firstOrNull { it.kind == StreamKind.Video }?.dynamicRange

    /** The best of [files]' dynamic ranges, Dolby Vision first, SDR last. */
    private fun bestRange(files: List<MediaFile>): DynamicRange? =
        files.mapNotNull(::rangeOf).filter { it in RANGES }.maxByOrNull(RANGES::indexOf)

    /**
     * The tracks a file starts with for [profile]: its choice for the movie
     * or show, else its defaults. The audio is a pick only when the choice
     * names one; else each player gets its best track of that language.
     */
    fun withChoices(item: ItemDetail, file: MediaFile, profile: Profile): MediaFile {
        val owner = if (item.kind == ItemKind.Episode) item.grandparentId else item.id
        val choice = owner?.let { profile.streamChoices[it] }
        val audio = file.streams.filter { it.kind == StreamKind.Audio }
        val subtitles = file.streams.filter { it.kind == StreamKind.Subtitle && it.textBased == true }
        val defaults = profile.trackDefaults
        // A choice without audio (subtitles picked alone) leaves it to the defaults.
        val pickedAudio = choice?.audioIndex?.let { index -> audio.firstOrNull { it.index == index } }
        val chosenAudio = pickedAudio ?: audio.firstOrNull { it.language == defaults.audioLanguage } ?: audio.firstOrNull { it.default }
        if (choice != null) {
            return file.copy(
                selectedAudio = chosenAudio?.index,
                audioPicked = pickedAudio != null,
                selectedSubtitle = choice.subtitleIndex?.takeIf { index -> file.streams.any { it.index == index } },
            )
        }
        val wanted = defaults.subtitleLanguage ?: chosenAudio?.language
        val subtitle = when (defaults.subtitleMode) {
            SubtitleMode.Always -> subtitles.firstOrNull { it.language == wanted }
            SubtitleMode.Foreign -> if (chosenAudio?.language != wanted) subtitles.firstOrNull { it.language == wanted } else null
            SubtitleMode.Forced -> subtitles.firstOrNull { it.forced && it.language == chosenAudio?.language }
            else -> null
        }
        return file.copy(selectedAudio = chosenAudio?.index, selectedSubtitle = subtitle?.index)
    }

    // ---------------------------------------------------------------- collections

    fun collectionItems(collection: CollectionEntry): List<ItemDetail> {
        val items = if (collection.smartFilter != null) {
            filter(collection.libraryId, collection.smartFilter, profile = null)
        } else {
            collection.itemIds.mapNotNull(catalog::item)
        }
        return when (collection.sort) {
            CollectionSort.Title -> items.sortedBy { it.sortTitle.lowercase() }
            CollectionSort.Release -> items.sortedBy { it.releaseDate.orEmpty() }
            else -> items
        }
    }

    fun collectionSummary(collection: CollectionEntry): CollectionSummary {
        val items = collectionItems(collection)
        return CollectionSummary(
            id = collection.id,
            libraryId = collection.libraryId,
            title = collection.title,
            summary = collection.summary,
            automatic = collection.automatic,
            sort = collection.sort,
            smartFilter = collection.smartFilter,
            itemCount = items.size,
            posterUrl = "/api/collections/${collection.id}/images/poster?v=${collection.createdAt}",
            backdropUrl = "/api/collections/${collection.id}/images/backdrop?v=${collection.createdAt}",
            createdAt = collection.createdAt,
            updatedAt = collection.createdAt,
        )
    }

    fun collectionDetail(collection: CollectionEntry, profile: Profile): CollectionDetail {
        val s = collectionSummary(collection)
        return CollectionDetail(
            id = s.id,
            libraryId = s.libraryId,
            title = s.title,
            summary = s.summary,
            automatic = s.automatic,
            sort = s.sort,
            smartFilter = s.smartFilter,
            itemCount = s.itemCount,
            posterUrl = s.posterUrl,
            backdropUrl = s.backdropUrl,
            createdAt = s.createdAt,
            updatedAt = s.updatedAt,
            items = collectionItems(collection).map { summary(it, profile) },
        )
    }

    /**
     * What a smart filter finds among [libraryId]'s top-level items, in the
     * Plex-compatible API's syntax as far as the fixtures use it: `genre`,
     * `year`, `decade`, `unwatched=1` and `sort=<field>[:desc]` (title, year).
     */
    fun filter(libraryId: Long, filter: String, profile: Profile?): List<ItemDetail> {
        val terms = filter.split('&').filter { '=' in it }.associate { it.substringBefore('=') to it.substringAfter('=') }
        var items = catalog.topLevel[libraryId].orEmpty().mapNotNull(catalog::item)
        terms["genre"]?.let { genre -> items = items.filter { genre in it.genres } }
        terms["year"]?.toIntOrNull()?.let { year -> items = items.filter { it.year == year } }
        terms["decade"]?.toIntOrNull()?.let { decade -> items = items.filter { (it.year ?: 0) / 10 * 10 == decade } }
        if (terms["unwatched"] == "1" && profile != null) items = items.filter { !state(it, profile).watched }
        val sort = terms["sort"]
        if (sort != null) {
            val field = sort.substringBefore(':')
            val sorted = when (field) {
                "year" -> items.sortedBy { it.year ?: 0 }
                else -> items.sortedBy { it.sortTitle.lowercase() }
            }
            items = if (sort.endsWith(":desc")) sorted.reversed() else sorted
        }
        return items.take(5_000)
    }

    // ---------------------------------------------------------------- playlists

    /** A playlist's entries: a smart one's are its filter's leaves, keyed by item. */
    fun playlistEntries(playlist: Playlist, profile: Profile): List<Pair<Long, Long>> {
        val filter = playlist.smartFilter ?: return playlist.entries.toList()
        val library = playlist.libraryId ?: return emptyList()
        return filter(library, filter, profile).flatMap { catalog.leaves(it.id) }.take(5_000).map { it to it }
    }

    fun playlistDetail(playlist: Playlist, profile: Profile): PlaylistDetail {
        val entries = playlistEntries(playlist, profile).mapNotNull { (entry, item) ->
            catalog.item(item)?.let { PlaylistEntry(entry, summary(it, profile)) }
        }
        return PlaylistDetail(
            id = playlist.id,
            title = playlist.title,
            kind = playlist.kind,
            itemCount = entries.size,
            durationMs = entries.sumOf { it.item.durationMs ?: 0L },
            artwork = entries.mapNotNull { it.item.posterUrl }.distinct().take(4),
            smartFilter = playlist.smartFilter,
            libraryId = playlist.libraryId,
            createdAt = playlist.createdAt,
            updatedAt = playlist.updatedAt,
            entries = entries,
        )
    }

    /** What a playlist of [kind] may hold. */
    fun fits(kind: PlaylistKind, item: ItemDetail): Boolean = when (kind) {
        PlaylistKind.Video -> item.kind == ItemKind.Movie || item.kind == ItemKind.Episode
        PlaylistKind.Audio -> item.kind == ItemKind.Track
        else -> false
    }

    // ---------------------------------------------------------------- people, history

    fun me(profile: Profile, scope: String?): Me = Me(
        id = profile.id,
        username = profile.username,
        name = profile.name,
        displayName = profile.name,
        isAdmin = false,
        managed = profile.managed,
        home = true,
        hasPin = profile.pin != null,
        avatarUrl = profile.avatarUrl,
        avatarColor = profile.avatarColor,
        allLibraries = true,
        libraryIds = emptyList(),
        createdAt = BASE_TIME - 500 * Catalog.DAY,
        audioLanguage = profile.trackDefaults.audioLanguage,
        subtitleLanguage = profile.trackDefaults.subtitleLanguage,
        subtitleMode = profile.trackDefaults.subtitleMode,
        hasPassword = profile.hasPassword,
        scope = scope,
    )

    fun history(record: HistoryRecord, profile: Profile): HistoryEntry? {
        val item = catalog.item(record.itemId) ?: return null
        return HistoryEntry(
            id = record.id,
            viewedAt = record.viewedAt,
            device = record.device,
            user = HistoryUser(profile.id, profile.name, profile.avatarUrl, profile.avatarColor),
            item = summary(item, profile),
        )
    }

    private companion object {
        /** Dynamic ranges, worst to best. */
        val RANGES = listOf(DynamicRange.Sdr, DynamicRange.Hlg, DynamicRange.Hdr10, DynamicRange.Hdr10Plus, DynamicRange.DolbyVision)
    }
}
