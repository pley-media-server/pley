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
 * The rail's entries between the avatar and Settings (design spec §2.1):
 * Search, Home, the first [MAX_LIBRARIES] libraries in the server's order,
 * Playlists and Collections. With more libraries, a "Libraries" entry,
 * which lists them all, takes Collections' place (collections stay on
 * each library's Collections tab). Live TV, where it has channels
 * ([liveTv], as the web's sidebar shows it), comes after Home, and the way
 * into requests ([requests], [requestsItem]) before it; while music has a
 * queue ([nowPlaying]), Now Playing comes last, and while a USB drive is
 * plugged into the TV ([drive]), USB drive after it, as on the Samsung
 * app. Nothing makes way for another: the entries scroll when they don't
 * fit ([railScroll]).
 */
fun railItems(
    libraries: List<Library>,
    nowPlaying: Boolean = false,
    liveTv: Boolean = false,
    requests: RailItem? = null,
    drive: Boolean = false,
): List<RailItem> {
    val main = mainItems(libraries)
    return buildList {
        addAll(main.take(2))
        if (requests != null) add(requests)
        if (liveTv) add(LiveTvItem)
        addAll(main.drop(2))
        if (nowPlaying) add(NowPlayingItem)
        if (drive) add(DriveItem)
    }
}

private fun mainItems(libraries: List<Library>): List<RailItem> = buildList {
    add(RailItem("search", RailSlot.Search, PleyKey.Search()))
    add(RailItem("home", RailSlot.Home, PleyKey.Home))
    libraries.take(MAX_LIBRARIES).forEach { library ->
        add(RailItem("library:${library.id}", RailSlot.Library, PleyKey.Library(library.id), library.name, library.kind))
    }
    if (libraries.size > MAX_LIBRARIES) add(LibrariesItem)
    add(RailItem("playlists", RailSlot.Playlists, PleyKey.Playlists))
    if (libraries.size <= MAX_LIBRARIES) {
        // A library's collections; the first video library's, as the web's Collections link.
        val first = libraries.firstOrNull { it.kind != LibraryKind.Music } ?: libraries.firstOrNull()
        if (first != null) add(RailItem("collections", RailSlot.Collections, PleyKey.Collections(first.id)))
    }
}

/** Every library: the Libraries page. */
private val LibrariesItem = RailItem("libraries", RailSlot.Libraries, PleyKey.Libraries)

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

/** At most this many libraries in the rail, then "Libraries" (design spec §2.1). */
const val MAX_LIBRARIES = 6
