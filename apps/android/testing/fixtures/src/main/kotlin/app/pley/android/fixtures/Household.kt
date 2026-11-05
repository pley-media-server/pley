package app.pley.android.fixtures

import app.pley.android.model.PlaylistKind
import app.pley.android.model.ProfileNeeds
import app.pley.android.model.StreamChoice
import app.pley.android.model.SubtitleMode
import app.pley.android.model.TrackDefaults
import app.pley.android.model.UserState

/** A profile of the household, and everything of its own that apps change. */
internal class Profile(
    val id: Long,
    val username: String,
    val name: String,
    val avatarColor: String,
    val hasAvatar: Boolean,
    /** Its PIN, if it has one. */
    val pin: String?,
    val hasPassword: Boolean,
    val managed: Boolean,
) {
    var trackDefaults = TrackDefaults(audioLanguage = "eng", subtitleLanguage = null, subtitleMode = SubtitleMode.Forced)

    /** The language Pley is in for it ("fr"); `null`: the server's. */
    var language: String? = null

    /** Playable items' states (positions, watched marks) and any item's own rating. */
    val states = HashMap<Long, UserState>()

    /** Track choices, by movie or show. */
    val streamChoices = HashMap<Long, StreamChoice>()

    /** My List, by item, in the order added. */
    val watchlist = LinkedHashMap<Long, Listed>()

    /** Movies and shows taken out of Continue Watching, and when (unix seconds). */
    val hiddenFromContinue = HashMap<Long, Long>()
    val history = mutableListOf<HistoryRecord>()

    /** Wrong PINs typed for this profile, by time (unix seconds). */
    val wrongPins = mutableListOf<Long>()

    /** Moves on with every change, so listings' ETags change too. */
    var version = 0L

    val avatarUrl: String? get() = if (hasAvatar) "/api/users/$id/avatar?v=$BASE_TIME" else null

    /** What switching to it takes from the app. */
    val needs: ProfileNeeds
        get() = when {
            pin != null -> ProfileNeeds.Pin
            hasPassword && id != FixtureIds.ALEX -> ProfileNeeds.Password
            else -> ProfileNeeds.None
        }

    fun state(item: Long): UserState = states[item] ?: UserState()

    fun update(item: Long, change: (UserState) -> UserState) {
        states[item] = change(state(item))
        version++
    }
}

/**
 * An item of My List: when it was added (unix seconds), and whether it was
 * watched then (a movie leaves the list once watched after that).
 */
internal class Listed(val itemId: Long, val addedAt: Long, var watchedThen: Boolean)

internal class HistoryRecord(val id: Long, val itemId: Long, val viewedAt: Long, val device: String?)

/** A playlist; a smart one has [smartFilter] and [libraryId] and no entries of its own. */
internal class Playlist(
    val id: Long,
    val owner: Long,
    var title: String,
    val kind: PlaylistKind,
    var smartFilter: String?,
    val libraryId: Long?,
    val createdAt: Long,
) {
    /** (entry id, item id), in order. */
    val entries = mutableListOf<Pair<Long, Long>>()
    var updatedAt: Long = createdAt
}

/**
 * The household (three profiles) and what they did before the app came:
 * Alex has things in progress, watched, rated, in the history, in three
 * playlists and in My List.
 */
internal class Household(catalog: Catalog) {
    val profiles: Map<Long, Profile> = listOf(
        Profile(FixtureIds.ALEX, "alex", "Alex", "#38bdf8", hasAvatar = true, pin = null, hasPassword = true, managed = false),
        Profile(FixtureIds.SAM, "sam", "Sam", "#a3e635", hasAvatar = false, pin = FixtureIds.PIN, hasPassword = false, managed = true),
        Profile(FixtureIds.ROBIN, "robin", "Robin", "#f472b6", hasAvatar = true, pin = null, hasPassword = true, managed = false),
    ).associateBy { it.id }

    val playlists = LinkedHashMap<Long, Playlist>()
    private var nextPlaylist = 1L
    private var nextEntry = 1L
    private var nextHistory = 1L

    init {
        seed(catalog)
    }

    fun newPlaylist(owner: Long, title: String, kind: PlaylistKind, smartFilter: String?, libraryId: Long?, now: Long): Playlist {
        val playlist = Playlist(nextPlaylist++, owner, title, kind, smartFilter, libraryId, now)
        playlists[playlist.id] = playlist
        return playlist
    }

    fun newEntry(): Long = nextEntry++

    fun addHistory(profile: Profile, item: Long, at: Long, device: String?) {
        profile.history.add(0, HistoryRecord(nextHistory++, item, at, device))
    }

    private fun seed(catalog: Catalog) {
        val alex = profiles.getValue(FixtureIds.ALEX)
        // In progress: a few movies and the fixture show's second episode.
        for ((n, id) in (7L..14L).withIndex()) {
            val duration = catalog.item(id)?.durationMs ?: continue
            alex.states[id] = UserState(positionMs = duration / (n + 2), lastPlayedAt = BASE_TIME - n * 3_600L)
        }
        alex.states[FixtureIds.SHOW_S1E1] = UserState(watched = true, playCount = 1, lastPlayedAt = BASE_TIME - 2 * 86_400L)
        alex.states[FixtureIds.SHOW_S1E2] = UserState(positionMs = 9_000, lastPlayedAt = BASE_TIME - 600)
        // Watched, some rated.
        for (id in 20L..80L step 3) {
            alex.states[id] = UserState(watched = true, playCount = 1, lastPlayedAt = BASE_TIME - id * 86_400L, rating = if (id % 2 == 0L) 8.0 else null)
        }
        for ((n, id) in ((20L..80L step 3).toList() + FixtureIds.SHOW_S1E1).withIndex()) {
            val at = alex.state(id).lastPlayedAt ?: (BASE_TIME - n * 86_400L)
            alex.history += HistoryRecord(nextHistory++, id, at, if (n % 2 == 0) "Pley on Android" else "Chrome on Windows")
        }
        alex.history.sortByDescending { it.viewedAt }

        val video = newPlaylist(FixtureIds.ALEX, "Favourites", PlaylistKind.Video, null, null, BASE_TIME - 10 * 86_400L)
        for (id in listOf(1L, 2L, 15L, 16L, 17L, 30L, 31L, 32L, 101L, 202L, 303L, FixtureIds.SHOW_S1E1)) {
            video.entries += newEntry() to id
        }
        val audio = newPlaylist(FixtureIds.ALEX, "Road Trip", PlaylistKind.Audio, null, null, BASE_TIME - 5 * 86_400L)
        for (id in catalog.leaves(FixtureIds.ALBUM) + catalog.leaves(FixtureIds.ALBUM + 5)) {
            audio.entries += newEntry() to id
        }
        newPlaylist(FixtureIds.ALEX, "Unwatched Comedies", PlaylistKind.Video, SMART_PLAYLIST_FILTER, FixtureIds.MOVIES, BASE_TIME - 86_400L)

        // My List: two shows and three movies, oldest first.
        val listed = listOf(15L, catalog.topLevel[FixtureIds.SHOWS].orEmpty().getOrNull(1), 16L, FixtureIds.SHOW, 17L)
        for ((n, id) in listed.filterNotNull().withIndex()) {
            alex.watchlist[id] = Listed(id, BASE_TIME - (10 - n) * 3_600L, watchedThen = false)
        }
    }

    companion object {
        const val SMART_PLAYLIST_FILTER = "genre=Comedy&unwatched=1&sort=year:desc"
    }
}
