package app.pley.android.screens.playlists

import app.pley.android.common.UiText
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.PlaylistKind
import app.pley.android.model.PlaylistSummary
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.cardSubtitle
import app.pley.android.model.format.episodeLabel
import app.pley.android.model.format.formatDuration
import app.pley.android.model.format.seasonTitle
import app.pley.android.screens.R
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

/**
 * The rules and words playlists share across their pages and sheets
 * (apps/web/src/pages/playlists, components/lists).
 */
object Playlists {
    /** A title is at most this long, as on the web. */
    const val TITLE_MAX = 200

    /** "12 items", "3h 12m" (its duration in [words]); just the count for a playlist without a duration. */
    fun metaTexts(itemCount: Int, durationMs: Long, words: FormatWords): List<UiText> = listOfNotNull(
        UiText.plural(R.plurals.playlists_count_items, itemCount),
        formatDuration(durationMs, words).takeIf { it.isNotEmpty() }?.let(UiText::raw),
    )

    /** What a playlist of [kind]'s items plays: music for artists, albums and tracks, else video. */
    fun kindOf(kind: ItemKind): PlaylistKind = when (kind) {
        ItemKind.Artist, ItemKind.Album, ItemKind.Track -> PlaylistKind.Audio
        else -> PlaylistKind.Video
    }

    /** [ids] with the one at [from] moved to [to] (unchanged when [from] is out of range), as the web's `moved`. */
    fun <T> moved(ids: List<T>, from: Int, to: Int): List<T> {
        if (from !in ids.indices) return ids
        val next = ids.toMutableList()
        val id = next.removeAt(from)
        next.add(to.coerceIn(0, next.size), id)
        return next
    }

    /** What is wrong with [title] as a playlist's title, or `null` when it will do. */
    fun check(title: String): TitleProblem? {
        val t = title.trim()
        return when {
            t.isEmpty() -> TitleProblem.Empty
            t.length > TITLE_MAX -> TitleProblem.TooLong
            else -> null
        }
    }

    /**
     * How many entries adding [items] makes: one for a movie, episode or
     * track, a show's, season's, album's or artist's episodes or tracks
     * for the others. `null` when that isn't known (a container without
     * its count).
     */
    fun entriesOf(items: List<ItemSummary>): Int? {
        var total = 0
        for (item in items) {
            total += when (item.kind) {
                ItemKind.Movie, ItemKind.Episode, ItemKind.Track, ItemKind.Extra -> 1
                else -> item.leafCount ?: return null
            }
        }
        return total
    }

    /**
     * A row's two lines, as the web's playlist page writes them: an
     * episode's title over its show and number, a season's name over its
     * show, else the title over the year (or what a card says), in [words].
     */
    fun rowTitles(item: ItemSummary, words: FormatWords): Pair<String, String> = when (item.kind) {
        ItemKind.Episode -> item.title to listOf(item.grandparentTitle.orEmpty(), episodeLabel(item, words)).filter { it.isNotEmpty() }.joinToString(" · ")
        ItemKind.Season -> seasonTitle(item.title, item.index, words) to cardSubtitle(item, words)
        else -> item.title to cardSubtitle(item, words)
    }

    /** A playlist as its card and the sheets list it, its duration in [words]. */
    fun card(summary: PlaylistSummary, origin: String, words: FormatWords): PlaylistCard = PlaylistCard(
        id = summary.id,
        title = summary.title,
        metaTexts = metaTexts(summary.itemCount, summary.durationMs, words),
        video = summary.kind != PlaylistKind.Audio,
        smart = !summary.smartFilter.isNullOrEmpty(),
        artwork = summary.artwork,
        origin = origin,
    )
}

/** Why a title won't do. */
enum class TitleProblem { Empty, TooLong }

/**
 * A playlist's card: its title, "12 items · 3h 12m" ([metaTexts]),
 * whether it plays video or music, the Smart badge, and the pictures of
 * its first entries for the mosaic ([artwork], the server's addresses, on
 * [origin]).
 */
data class PlaylistCard(
    val id: Long,
    val title: String,
    val metaTexts: List<UiText>,
    val video: Boolean,
    val smart: Boolean,
    val artwork: List<String>,
    val origin: String,
)

/**
 * Work started from a page or sheet that may close before it ends: a
 * change to send, a playlist to make. It runs to its end on the main
 * thread's loop, whatever happens to the screen.
 */
internal val PlaylistWork: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
