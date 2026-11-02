package app.pley.android.screens

import androidx.navigation3.runtime.NavKey
import app.pley.android.model.TitleKind
import kotlinx.serialization.Serializable

/**
 * Every place in the app, for both interfaces: the phone keeps one back
 * stack per tab of these, the TV one. They are saved with the back stack
 * (process death, rotation), so they hold ids and choices only, never
 * what the server answered.
 */
@Serializable
sealed interface PleyKey : NavKey {
    // Getting in.

    /** The saved servers and the ones found on the network, and an address to type. */
    @Serializable
    data object Servers : PleyKey

    /** Signing in to [server] in the browser (phones). */
    @Serializable
    data class SignIn(val server: String) : PleyKey

    /** Signing in to [server] with a code shown on screen (TVs, and phones without a browser). */
    @Serializable
    data class Link(val server: String) : PleyKey

    /**
     * "Who's watching?": the household's profiles, and a PIN pad. [pin]: a
     * TV's start as the profile it was left on, whose pad opens first.
     */
    @Serializable
    data class Profiles(val pin: Boolean = false) : PleyKey

    // Browsing.

    @Serializable
    data object Home : PleyKey

    @Serializable
    data object Libraries : PleyKey

    /**
     * One library, on [tab] (recommended, library or collections); `null`:
     * on the tab it was left on this device, else Library. [view] is a
     * sort to show it in ("sort=added&order=desc", a row's See all), in
     * place of the one it was left in.
     */
    @Serializable
    data class Library(val libraryId: Long, val tab: LibraryTab? = null, val view: String? = null) : PleyKey

    /** A library's collections. */
    @Serializable
    data class Collections(val libraryId: Long) : PleyKey

    @Serializable
    data class Collection(val collectionId: Long) : PleyKey

    /** A movie, show, season, episode or extra. */
    @Serializable
    data class Item(val itemId: Long) : PleyKey

    @Serializable
    data class Person(val personId: Long) : PleyKey

    @Serializable
    data class Artist(val itemId: Long) : PleyKey

    /** An album; [trackId] is the track it was opened from, highlighted in its list. */
    @Serializable
    data class Album(val itemId: Long, val trackId: Long? = null) : PleyKey

    @Serializable
    data class Search(val query: String = "") : PleyKey

    @Serializable
    data object Playlists : PleyKey

    @Serializable
    data class Playlist(val playlistId: Long) : PleyKey

    /** Live TV: the guide and the channels (the player is a window of its own: PlayerLauncher.watchLive). */
    @Serializable
    data object LiveTv : PleyKey

    // Requests.

    /** Discover: what TMDB has to ask for. */
    @Serializable
    data object Discover : PleyKey

    /** Every title TMDB finds for [query] (the See all of search's Discover row). */
    @Serializable
    data class DiscoverResults(val query: String) : PleyKey

    /** Every title of one of Discover's lists (a row's See all, on TVs). */
    @Serializable
    data class DiscoverList(val list: app.pley.android.model.DiscoverList) : PleyKey

    /** A movie or show found on TMDB; [pick] opens its season picker once the page shows (Request more seasons). */
    @Serializable
    data class DiscoverTitle(val kind: TitleKind, val tmdbId: Long, val pick: Boolean = false) : PleyKey

    /** Your requests; for managers, everyone's (Requests), opening on Needs attention. */
    @Serializable
    data object Requests : PleyKey

    // Yours.

    /** The profile's own page: switching profiles, track defaults, history, settings. */
    @Serializable
    data object Me : PleyKey

    /** What this profile watched, newest first. */
    @Serializable
    data object History : PleyKey

    /** What was downloaded to this device, to watch without the server. */
    @Serializable
    data object Downloads : PleyKey

    @Serializable
    data object Settings : PleyKey

    /** What this device decodes and shows, for when something doesn't play. */
    @Serializable
    data object Diagnostics : PleyKey

    /** Music playing now, with its queue. */
    @Serializable
    data object NowPlaying : PleyKey

    // Sheets, over the screen they were opened from.

    /**
     * What can be done with an item: play from the start, mark watched, add
     * to a playlist… Opened from a Continue Watching tile ([continueWatching]),
     * also taking it out of that row.
     */
    @Serializable
    data class ItemActions(val itemId: Long, val continueWatching: Boolean = false) : PleyKey

    @Serializable
    data class AddToPlaylist(val itemIds: List<Long>) : PleyKey

    /** A new playlist, with [itemIds] in it. */
    @Serializable
    data class NewPlaylist(val itemIds: List<Long> = emptyList()) : PleyKey

    /** Saving a library's sort and filters as a smart playlist. */
    @Serializable
    data class SaveSmartPlaylist(val libraryId: Long, val query: String) : PleyKey
}

/** The tabs of a library page, as on the web. */
@Serializable
enum class LibraryTab { Recommended, Library, Collections }

/** Whether [key] shows over the screen it was opened from rather than in its place. */
val PleyKey.isSheet: Boolean
    get() = this is PleyKey.ItemActions || this is PleyKey.AddToPlaylist ||
        this is PleyKey.NewPlaylist || this is PleyKey.SaveSmartPlaylist
