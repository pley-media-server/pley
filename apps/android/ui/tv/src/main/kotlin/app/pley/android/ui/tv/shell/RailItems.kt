package app.pley.android.ui.tv.shell

import androidx.compose.runtime.Immutable
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.requests.RequestsEntry

/** The kinds of place the rail goes to. */
enum class RailSlot { Search, Home, Discover, Requests, LiveTv, Library, Libraries, Playlists, Collections, NowPlaying, Drive, Settings }

/** One rail entry: where it goes, for a library its name and kind, for requests what they carry. */
@Immutable
data class RailItem(
    /** Stable across library changes: "home", "library:3", "settings". */
    val id: String,
    val slot: RailSlot,
    val key: PleyKey,
    /** A library's name, as the server has it; the other slots have their own strings. */
    val name: String? = null,
    val libraryKind: LibraryKind? = null,
    /** The way into requests: whose requests it leads to (the managers' Requests), and the number that waits there. */
    val requests: RequestsEntry? = null,
) {
    val icon: PleyIcon
        get() = when (slot) {
            RailSlot.Search -> PleyIcons.Search
            RailSlot.Home -> PleyIcons.House
            RailSlot.Discover -> PleyIcons.Compass
            RailSlot.Requests -> PleyIcons.ListVideo
            RailSlot.LiveTv -> PleyIcons.TvMinimal
            RailSlot.Library -> when (libraryKind) {
                LibraryKind.Movies -> PleyIcons.Clapperboard
                LibraryKind.Shows -> PleyIcons.Tv
                LibraryKind.Music -> PleyIcons.Music
                else -> PleyIcons.Film
            }
            RailSlot.Libraries -> PleyIcons.Layers
            RailSlot.Playlists -> PleyIcons.ListMusic
            RailSlot.Collections -> PleyIcons.Layers
            RailSlot.NowPlaying -> PleyIcons.AudioLines
            RailSlot.Drive -> PleyIcons.Usb
            RailSlot.Settings -> PleyIcons.Settings
        }
}

/**
 * The rail's entries above Settings (design spec §2.1): Search, Home, the
 * first [MAX_LIBRARIES] libraries in the server's order, Playlists and
 * Collections. Between the avatar and Settings there is room for seven
 * entries, so with more libraries a "Libraries" entry, which lists them
 * all, takes Collections' place (collections stay on each library's
 * Collections tab). Live TV, where it has channels ([liveTv], as the
 * web's sidebar shows it), comes after Home, and the way into requests
 * ([requests], [requestsItem]) before it; while music has a queue
 * ([nowPlaying]), Now Playing comes last, and while a USB drive is plugged
 * into the TV ([drive]), USB drive after it, as on the Samsung app. Each
 * takes room as it is added ([roomIn]).
 */
fun railItems(
    libraries: List<Library>,
    nowPlaying: Boolean = false,
    liveTv: Boolean = false,
    requests: RailItem? = null,
    drive: Boolean = false,
): List<RailItem> {
    var items = mainItems(libraries)
    if (liveTv) items = roomIn(items).let { listOf(it[0], it[1], LiveTvItem) + it.drop(2) }
    if (requests != null) items = roomIn(items).let { listOf(it[0], it[1], requests) + it.drop(2) }
    if (nowPlaying) items = roomIn(items) + NowPlayingItem
    if (drive) items = roomIn(items) + DriveItem
    return items
}

/**
 * [items] with room for one more: as they are when there is room, else
 * without Collections, else without the last library ("Libraries" lists
 * it); with no "Libraries" yet, the last two libraries make way for it.
 */
private fun roomIn(items: List<RailItem>): List<RailItem> {
    if (items.size < MAX_ITEMS) return items
    if (items.any { it.slot == RailSlot.Collections }) return items.filter { it.slot != RailSlot.Collections }
    val libraries = items.filter { it.slot == RailSlot.Library }
    if (items.any { it.slot == RailSlot.Libraries }) return items - libraries.last()
    val leaving = libraries.takeLast(2)
    val at = items.indexOf(leaving.first())
    val kept = items - leaving.toSet()
    return kept.take(at) + RailItem("libraries", RailSlot.Libraries, PleyKey.Libraries) + kept.drop(at)
}

private fun mainItems(libraries: List<Library>): List<RailItem> = buildList {
    add(RailItem("search", RailSlot.Search, PleyKey.Search()))
    add(RailItem("home", RailSlot.Home, PleyKey.Home))
    libraries.take(MAX_LIBRARIES).forEach { library ->
        add(RailItem("library:${library.id}", RailSlot.Library, PleyKey.Library(library.id), library.name, library.kind))
    }
    if (libraries.size > MAX_LIBRARIES) add(RailItem("libraries", RailSlot.Libraries, PleyKey.Libraries))
    add(RailItem("playlists", RailSlot.Playlists, PleyKey.Playlists))
    if (libraries.size <= MAX_LIBRARIES) {
        // A library's collections; the first video library's, as the web's Collections link.
        val first = libraries.firstOrNull { it.kind != LibraryKind.Music } ?: libraries.firstOrNull()
        if (first != null) add(RailItem("collections", RailSlot.Collections, PleyKey.Collections(first.id)))
    }
}

/** Live TV, where it has channels. */
val LiveTvItem = RailItem("live-tv", RailSlot.LiveTv, PleyKey.LiveTv)

/**
 * The rail's way into requests, as the Samsung and LG apps have it:
 * Discover where the profile may ask, else its requests (the managers'
 * Requests) where it has any to see, else none. It carries their number.
 */
fun requestsItem(entry: RequestsEntry): RailItem? = when {
    entry.discover -> RailItem("discover", RailSlot.Discover, PleyKey.Discover, requests = entry)
    entry.requests -> RailItem("requests", RailSlot.Requests, PleyKey.Requests, requests = entry)
    else -> null
}

/** Music's Now Playing, while there is music. */
val NowPlayingItem = RailItem("now-playing", RailSlot.NowPlaying, PleyKey.NowPlaying)

/** The USB drive plugged into the TV, while one is: its top folder. */
val DriveItem = RailItem("drive", RailSlot.Drive, PleyKey.Drive())

/** Settings, pinned at the rail's foot. */
val SettingsItem = RailItem("settings", RailSlot.Settings, PleyKey.Settings)

/** The avatar's id: it opens Who's watching?. */
const val PROFILE_ITEM = "profile"

/**
 * Which entry is current for a back stack whose first page is [root]: the
 * entry that opened it, "libraries" for a library not in the rail, or
 * `null` (Who's watching?, sign-in).
 */
fun railCurrent(root: PleyKey, items: List<RailItem>): String? = when (root) {
    is PleyKey.Library -> items.firstOrNull { it.key == PleyKey.Library(root.libraryId) }?.id
        ?: items.firstOrNull { it.slot == RailSlot.Libraries }?.id
    is PleyKey.Collections -> items.firstOrNull { it.slot == RailSlot.Collections }?.id
        ?: railCurrent(PleyKey.Library(root.libraryId), items)
    is PleyKey.Search -> "search"
    // Discover's pages, and the requests reached from it, under whichever of the two the rail has.
    PleyKey.Discover, PleyKey.Requests, is PleyKey.DiscoverList, is PleyKey.DiscoverTitle ->
        (items.firstOrNull { it.key == root } ?: items.firstOrNull { it.slot == RailSlot.Discover || it.slot == RailSlot.Requests })?.id
    is PleyKey.Drive -> DriveItem.id
    PleyKey.Settings -> SettingsItem.id
    is PleyKey.Profiles -> PROFILE_ITEM
    else -> items.firstOrNull { it.key == root }?.id
}

/** At most this many libraries in the rail (design spec §2.1). */
const val MAX_LIBRARIES = 3

/** Entries between the avatar and Settings, at most: their 56 dp pitch fills the rail. */
const val MAX_ITEMS = 7
