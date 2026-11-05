package app.pley.android.screens.home

import app.pley.android.common.AppLanguage
import app.pley.android.common.UiText
import app.pley.android.model.Hub
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.format.cardSubtitle
import app.pley.android.model.format.episodeLabel
import app.pley.android.model.format.formatRuntime
import app.pley.android.model.format.formatTimeLeft
import app.pley.android.model.format.isInProgress
import app.pley.android.model.format.pluralize
import app.pley.android.model.format.progressFraction
import app.pley.android.model.format.qualityBadges
import app.pley.android.screens.LibraryTab
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R
import java.util.Locale

/**
 * Home as the screens draw it (design spec §4.1, §8), the same for the
 * phone and the TV. Worked out off the main thread from what the
 * repositories hold. Whether it is the server's latest is apart from it
 * ([HomeViewModel.status]), so the server saying again what it said
 * before changes nothing here, and redraws nothing.
 */
sealed interface HomeState {
    /** Nothing known yet: what was saved is still being read, or nothing was. */
    data object Loading : HomeState

    /** Home couldn't be loaded and nothing was saved: "Couldn't load your home screen". */
    data class Failed(val error: Throwable) : HomeState

    /** The profile sees no library: "No libraries yet". */
    data object NoLibraries : HomeState

    /** Libraries, but nothing in them yet: "Nothing here yet", or "Scanning your libraries…" while [scanning]. */
    data class Empty(val scanning: Boolean) : HomeState

    data class Ready(
        /** The server's origin, which the rows' picture addresses are on. */
        val origin: String,
        /** The rows with something in them, in the server's order. */
        val rows: List<HomeRow>,
        /** What the TV's hero shows; `null` with nothing to show. */
        val hero: HomeInfo?,
    ) : HomeState
}

/** How a row's tiles are shaped: the web's hubAspect (apps/web/src/components/HubRow.tsx). */
enum class RowShape {
    /** 16:9 stills: Continue Watching, and rows of episodes. */
    Landscape,

    /** Movies, shows and seasons. */
    Poster,

    /** Music. */
    Square,
}

/** One row of Home: a hub of the server's. */
data class HomeRow(
    /** The hub's key ("continue_watching", "recently_added:3"): stable, the row's key on the page. */
    val key: String,
    /** "Continue Watching", "Recently Added in Movies", as the server says it. */
    val title: String,
    val shape: RowShape,
    val items: List<ItemSummary>,
    /** Where "See all" goes: the library, listed as the row is; `null` for rows without one. */
    val seeAll: PleyKey?,
) {
    val continueWatching: Boolean get() = key.startsWith(CONTINUE_PREFIX)
}

/**
 * What the TV says about one item, in its hero (design spec §2.2) or in
 * the info block over the rows (§4.1), with every label made already.
 */
data class HomeInfo(
    val item: ItemSummary,
    /** The title, or for an episode its show's: the episode is in [context]. */
    val title: String,
    /** Episodes: "S2 · E5 · The Long Night"; empty otherwise. */
    val context: String,
    val meta: HomeMeta,
    /** Up to the screen to clamp; empty when the server has none. */
    val summary: String,
    /** Played so far, 0 to 1; 0 when not in progress. */
    val progress: Float,
    /** "26 min left" while in progress; empty otherwise. */
    val timeLeft: String,
    /** Movies, episodes and extras that play themselves. */
    val video: Boolean,
    val watched: Boolean,
    /** Whether the details (summary, content rating, quality) are in: the info block may change once more until then. */
    val complete: Boolean,
) {
    /** Started and not finished: Resume, rather than Play. */
    val inProgress: Boolean get() = progress > 0f
}

/** The metadata line: year · content rating · runtime · ★ rating · quality badges, the parts that are known. */
data class HomeMeta(
    val year: String = "",
    /** A badge: "TV-14", "PG-13", "15". */
    val contentRating: String? = null,
    /** "58 min", "2h 02m"; for shows "3 seasons". */
    val runtime: String = "",
    /** The critics' rating, "7.4". */
    val rating: String? = null,
    /** "4K", "DV P8.1", "Atmos". */
    val badges: List<String> = emptyList(),
)

/** A library in the list of libraries (phone's Libraries tab, the TV's Libraries page). */
data class LibraryEntry(
    val library: Library,
    /** "412 movies", "38 shows", "120 artists", in English: the TV's, until it shows [countText]. */
    val count: String,
    /** The server is scanning it now. */
    val scanning: Boolean,
) {
    /** "412 movies", "38 shows", "120 artists", in the language shown. */
    val countText: UiText = libraryCountText(library)
}

internal const val CONTINUE_PREFIX = "continue"

/**
 * The rows of [hubs] that hold something, shaped as the web does: Continue
 * Watching as stills, music as squares, the rest by their first item.
 * [libraries] (when known) says which libraries hold music.
 */
fun homeRows(hubs: List<Hub>, libraries: List<Library>?): List<HomeRow> = hubs.mapNotNull { hub ->
    if (hub.items.isEmpty()) return@mapNotNull null
    HomeRow(
        key = hub.key,
        title = hub.title,
        shape = rowShape(hub, libraries?.firstOrNull { it.id == hub.libraryId }?.kind),
        items = hub.items,
        seeAll = seeAllOf(hub),
    )
}

/** The shape of [hub]'s tiles: the web's hubAspect, with a music library's rows square. */
fun rowShape(hub: Hub, libraryKind: LibraryKind? = null): RowShape = when {
    hub.key.startsWith(CONTINUE_PREFIX) -> RowShape.Landscape
    libraryKind == LibraryKind.Music -> RowShape.Square
    else -> when (hub.items.firstOrNull()?.kind) {
        ItemKind.Episode -> RowShape.Landscape
        ItemKind.Artist, ItemKind.Album, ItemKind.Track -> RowShape.Square
        else -> RowShape.Poster
    }
}

/**
 * Where a row's "See all" goes: its library's items in the row's order, as
 * the web's hubLink (recently added, recently released and top rated,
 * newest or best first). The library page opens on its Library tab.
 */
fun seeAllOf(hub: Hub): PleyKey? {
    val library = hub.libraryId ?: return null
    return seeAllView(hub.key)?.let { PleyKey.Library(library, LibraryTab.Library, it) }
}

/** The Library tab's sort a row of [hubKey] lists its items in, for its See all; `null` for rows without one. */
fun seeAllView(hubKey: String): String? = SEE_ALL_VIEWS.entries.firstOrNull { hubKey.startsWith(it.key) }?.value

private val SEE_ALL_VIEWS = mapOf(
    "recently_added" to "sort=added&order=desc",
    "recently_released" to "sort=release&order=desc",
    "top_rated" to "sort=rating&order=desc",
)

/**
 * The item the TV's hero shows: the first of Continue Watching, else the
 * newest video in the rows, else the newest item of any kind.
 */
fun heroOf(rows: List<HomeRow>): ItemSummary? {
    rows.firstOrNull { it.continueWatching }?.items?.firstOrNull()?.let { return it }
    val all = rows.flatMap { it.items }
    return all.filter { it.kind in VIDEO_KINDS }.maxByOrNull { it.addedAt } ?: all.maxByOrNull { it.addedAt }
}

private val VIDEO_KINDS = setOf(ItemKind.Movie, ItemKind.Show, ItemKind.Season, ItemKind.Episode)

/**
 * What the hero or the info block says about [item], with [detail] once
 * it is in: the summary of a movie, the content rating and the quality
 * badges come only with it.
 */
fun homeInfo(item: ItemSummary, detail: ItemDetail?, locale: Locale = AppLanguage.locale): HomeInfo {
    val details = detail?.takeIf { it.id == item.id }
    val episode = item.kind == ItemKind.Episode
    val show = item.grandparentTitle?.takeIf { episode && it.isNotEmpty() }
    val inProgress = isInProgress(item)
    val runtime = when (item.kind) {
        ItemKind.Show, ItemKind.Season, ItemKind.Artist, ItemKind.Album -> cardSubtitle(item, locale)
        else -> formatRuntime(item.durationMs ?: details?.durationMs)
    }
    return HomeInfo(
        item = item,
        title = show ?: item.title,
        context = if (episode) listOf(episodeLabel(item), item.title).filter { it.isNotEmpty() }.joinToString(" · ") else "",
        meta = HomeMeta(
            year = (item.year ?: details?.year)?.toString().orEmpty(),
            contentRating = details?.contentRating?.takeIf { it.isNotBlank() },
            runtime = if (item.kind == ItemKind.Show && runtime == item.year?.toString()) "" else runtime,
            rating = (item.rating ?: details?.rating)?.takeIf { it > 0 }?.let { String.format(locale, "%.1f", it) },
            badges = details?.let(::qualityBadges).orEmpty(),
        ),
        summary = (details?.summary ?: item.summary).orEmpty().trim(),
        progress = if (inProgress) progressFraction(item) else 0f,
        timeLeft = if (inProgress) formatTimeLeft(item.userState.positionMs, item.durationMs) else "",
        video = item.kind == ItemKind.Movie || episode || item.kind == ItemKind.Extra,
        watched = item.userState.watched,
        complete = details != null,
    )
}

/** "412 movies", "38 shows", "120 artists": what a library holds, by its kind, in English. See [libraryCountText]. */
fun libraryCount(library: Library, locale: Locale = AppLanguage.locale): String {
    val (one, many) = when (library.kind) {
        LibraryKind.Movies -> "movie" to "movies"
        LibraryKind.Shows -> "show" to "shows"
        LibraryKind.Music -> "artist" to "artists"
        LibraryKind.Unknown -> "item" to "items"
    }
    return pluralize(library.itemCount, one, many, locale)
}

/** "412 movies", "38 shows", "120 artists": what a library holds, by its kind. */
fun libraryCountText(library: Library): UiText = UiText.plural(
    when (library.kind) {
        LibraryKind.Movies -> R.plurals.library_entry_movies
        LibraryKind.Shows -> R.plurals.library_entry_shows
        LibraryKind.Music -> R.plurals.library_entry_artists
        LibraryKind.Unknown -> R.plurals.library_entry_items
    },
    library.itemCount,
)

/** The page an item opens: music has its own pages (P5), the rest the item page. */
fun pageOf(item: ItemSummary): PleyKey = when (item.kind) {
    ItemKind.Artist -> PleyKey.Artist(item.id)
    ItemKind.Album -> PleyKey.Album(item.id)
    ItemKind.Track -> item.parentId?.let { PleyKey.Album(it) } ?: PleyKey.Item(item.id)
    else -> PleyKey.Item(item.id)
}
