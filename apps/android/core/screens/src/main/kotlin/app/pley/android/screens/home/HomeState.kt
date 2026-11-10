package app.pley.android.screens.home

import app.pley.android.common.AppLanguage
import app.pley.android.common.UiText
import app.pley.android.model.Hub
import app.pley.android.model.HubKeys
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.cardSubtitle
import app.pley.android.model.format.episodeLabel
import app.pley.android.model.format.formatRuntime
import app.pley.android.model.format.formatTimeLeft
import app.pley.android.model.format.isInProgress
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
        /**
         * What the TV's hero shows when nothing is [featured]: the first of
         * Continue Watching, else the newest; `null` with nothing to show.
         */
        val hero: HomeInfo?,
        /** The banner's titles ([featuredOf]), in turn; empty without a backdrop to show. */
        val featured: List<Featured> = emptyList(),
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
    /** From the details: "Drama", "Thriller". */
    val genres: List<String> = emptyList(),
    /** Movies and shows, which go on My List. */
    val listable: Boolean = false,
    /** On the profile's My List, as the details say. */
    val inList: Boolean = false,
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

/**
 * A title the banner features ([featuredOf]), with what it says about it,
 * and why it is there: "New in Movies", or "On your list".
 */
data class Featured(
    val info: HomeInfo,
    val label: UiText,
)

/** A title [featuredOf] picked: new in its library, on My List ([fromList]), or one of its library's top rated ([topRated]). */
data class FeaturedPick(val item: ItemSummary, val fromList: Boolean, val topRated: Boolean = false)

/** How many titles the banner takes turns showing. */
const val FEATURED_COUNT = 6

/** One place in this many of the banner's is a top rated title's ([featuredOf]): the third and the sixth of six. */
const val TOP_RATED_EVERY = 3

/**
 * The titles Home's banner features. Mostly what is new, picked as the web
 * picks it (apps/web/src/lib/featured.ts): the newest unwatched movies and
 * shows with a backdrop, taken in turn from each library's Recently Added
 * (so movies and shows alternate), then from My List when that is too few.
 * When everything has been watched, the newest are featured all the same.
 *
 * Among them, in every third place, comes a title of the libraries' Top
 * Rated not yet watched ([topRated]), while there are some. Which ones
 * depends on [day] and on nothing else that changes, so the banner shows
 * the same titles all day, and a title watched or added moves no other.
 * The first two places are always the newest, which Home has at once: the
 * top rated come from the libraries a moment later, and never change the
 * title the banner opens on.
 */
fun featuredOf(rows: List<HomeRow>, topRated: List<ItemSummary> = emptyList(), day: Long = 0, count: Int = FEATURED_COUNT): List<FeaturedPick> {
    val unwatched = pickFeatured(rows, count) { hasArt(it) && !it.userState.watched }
    val newest = unwatched.ifEmpty { pickFeatured(rows, count, ::hasArt) }
    val taken = newest.mapTo(HashSet()) { it.item.id }
    val rated = topRated.asSequence()
        .filter { hasArt(it) && !it.userState.watched && it.id !in taken }
        .distinctBy { it.id }
        .sortedBy { dayOrder(it.id, day) }
        .iterator()
    if (!rated.hasNext()) return newest
    val fresh = newest.iterator()
    val picked = ArrayList<FeaturedPick>(count)
    while (picked.size < count && (fresh.hasNext() || rated.hasNext())) {
        val itsPlace = picked.size % TOP_RATED_EVERY == TOP_RATED_EVERY - 1
        picked += if (rated.hasNext() && (itsPlace || !fresh.hasNext())) FeaturedPick(rated.next(), fromList = false, topRated = true) else fresh.next()
    }
    return picked
}

/**
 * Where [id] comes among the top rated on [day]: an order that looks
 * shuffled, is the same all day, and in which a title's place doesn't
 * depend on which other titles there are (SplitMix64's mixing).
 */
private fun dayOrder(id: Long, day: Long): Long {
    var x = id * -0x61c8864680b583ebL + day * -0x4b47d0c5ae7e0c4bL
    x = (x xor (x ushr 30)) * -0x40a7b892e31b1a47L
    x = (x xor (x ushr 27)) * -0x6b2fb644ecceee15L
    return x xor (x ushr 31)
}

private fun hasArt(item: ItemSummary): Boolean = item.kind.listable && !item.backdropUrl.isNullOrEmpty()

private fun pickFeatured(rows: List<HomeRow>, count: Int, featurable: (ItemSummary) -> Boolean): List<FeaturedPick> {
    val sources = rows.filter { it.key.startsWith(RECENTLY_ADDED_PREFIX) }.map { row -> row.items.filter(featurable) }
    val picked = ArrayList<FeaturedPick>(count)
    val seen = HashSet<Long>()
    fun take(item: ItemSummary, fromList: Boolean) {
        if (picked.size < count && seen.add(item.id)) picked += FeaturedPick(item, fromList)
    }
    var i = 0
    while (picked.size < count && sources.any { i < it.size }) {
        sources.forEach { source -> source.getOrNull(i)?.let { take(it, fromList = false) } }
        i++
    }
    rows.firstOrNull { it.key == HubKeys.MY_LIST }?.items?.forEach { if (featurable(it)) take(it, fromList = true) }
    return picked
}

/**
 * What the banner says about why [pick] is there: "New in Movies" or "Top
 * rated in Movies" with its library's name ([libraries]), "New" or "Top
 * rated" without it.
 */
fun featuredLabel(pick: FeaturedPick, libraries: List<Library>?): UiText {
    if (pick.fromList) return UiText.res(R.string.home_featured_on_list)
    val library = libraries?.firstOrNull { it.id == pick.item.libraryId }?.name
    return when {
        pick.topRated -> if (library.isNullOrEmpty()) UiText.res(R.string.home_featured_top_rated) else UiText.res(R.string.home_featured_top_rated_in, library)
        library.isNullOrEmpty() -> UiText.res(R.string.home_featured_new)
        else -> UiText.res(R.string.home_featured_new_in, library)
    }
}

/** A library in the list of libraries (phone's Libraries tab, the TV's Libraries page). */
data class LibraryEntry(
    val library: Library,
    /** The server is scanning it now. */
    val scanning: Boolean,
) {
    /** "412 movies", "38 shows", "120 artists", in the language shown. */
    val countText: UiText = libraryCountText(library)
}

internal const val CONTINUE_PREFIX = "continue"
private const val RECENTLY_ADDED_PREFIX = "recently_added"

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
fun homeInfo(
    item: ItemSummary,
    detail: ItemDetail?,
    locale: Locale = AppLanguage.locale,
    words: FormatWords = AppLanguage.formatWords(),
): HomeInfo {
    val details = detail?.takeIf { it.id == item.id }
    val episode = item.kind == ItemKind.Episode
    val show = item.grandparentTitle?.takeIf { episode && it.isNotEmpty() }
    val inProgress = isInProgress(item)
    val runtime = when (item.kind) {
        ItemKind.Show, ItemKind.Season, ItemKind.Artist, ItemKind.Album -> cardSubtitle(item, words, locale)
        else -> formatRuntime(item.durationMs ?: details?.durationMs, words)
    }
    return HomeInfo(
        item = item,
        title = show ?: item.title,
        context = if (episode) listOf(episodeLabel(item, words), item.title).filter { it.isNotEmpty() }.joinToString(" · ") else "",
        meta = HomeMeta(
            year = (item.year ?: details?.year)?.toString().orEmpty(),
            contentRating = details?.contentRating?.takeIf { it.isNotBlank() },
            runtime = if (item.kind == ItemKind.Show && runtime == item.year?.toString()) "" else runtime,
            rating = (item.rating ?: details?.rating)?.takeIf { it > 0 }?.let { String.format(locale, "%.1f", it) },
            badges = details?.let(::qualityBadges).orEmpty(),
        ),
        summary = (details?.summary ?: item.summary).orEmpty().trim(),
        progress = if (inProgress) progressFraction(item) else 0f,
        timeLeft = if (inProgress) formatTimeLeft(item.userState.positionMs, item.durationMs, words) else "",
        video = item.kind == ItemKind.Movie || episode || item.kind == ItemKind.Extra,
        watched = item.userState.watched,
        complete = details != null,
        genres = details?.genres.orEmpty(),
        listable = item.kind.listable,
        inList = details?.inWatchlist == true,
    )
}

/** This Home with the hero and the banner's titles told what [details] (by id) say about them. */
fun HomeState.Ready.withDetails(details: Map<Long, ItemDetail>): HomeState.Ready = copy(
    hero = hero?.let { homeInfo(it.item, details[it.item.id]) },
    featured = featured.map { it.copy(info = homeInfo(it.info.item, details[it.info.item.id])) },
)

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
