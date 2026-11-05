package app.pley.android.screens.playlists

import app.pley.android.common.AppLanguage
import app.pley.android.common.UiText
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.PlaylistKind
import app.pley.android.model.PlaylistSummary
import app.pley.android.model.format.cardSubtitle
import app.pley.android.model.format.episodeLabel
import app.pley.android.model.format.formatDuration
import app.pley.android.model.format.pluralize
import app.pley.android.model.format.seasonTitle
import app.pley.android.screens.R
import java.util.Locale
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

    /** "12 items · 3h 12m", "1 item", in English: [metaTexts], joined. */
    fun meta(itemCount: Int, durationMs: Long, locale: Locale = AppLanguage.locale): String =
        listOf(pluralize(itemCount, "item", locale = locale), formatDuration(durationMs)).filter { it.isNotEmpty() }.joinToString(" · ")

    /** "12 items", "3h 12m"; just the count for a playlist without a duration. */
    fun metaTexts(itemCount: Int, durationMs: Long): List<UiText> =
        listOfNotNull(UiText.plural(R.plurals.playlists_count_items, itemCount), formatDuration(durationMs).takeIf { it.isNotEmpty() }?.let(UiText::raw))

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
     * show, else the title over the year (or what a card says).
     */
    fun rowTitles(item: ItemSummary, locale: Locale = AppLanguage.locale): Pair<String, String> = when (item.kind) {
        ItemKind.Episode -> item.title to listOf(item.grandparentTitle.orEmpty(), episodeLabel(item)).filter { it.isNotEmpty() }.joinToString(" · ")
        ItemKind.Season -> seasonTitle(item.title, item.index) to cardSubtitle(item, locale)
        else -> item.title to cardSubtitle(item, locale)
    }

    /** A playlist as its card and the sheets list it. */
    fun card(summary: PlaylistSummary, origin: String, locale: Locale = AppLanguage.locale): PlaylistCard = PlaylistCard(
        id = summary.id,
        title = summary.title,
        meta = meta(summary.itemCount, summary.durationMs, locale),
        metaTexts = metaTexts(summary.itemCount, summary.durationMs),
        video = summary.kind != PlaylistKind.Audio,
        smart = !summary.smartFilter.isNullOrEmpty(),
        artwork = summary.artwork,
        origin = origin,
    )
}

/** Why a title won't do. */
enum class TitleProblem { Empty, TooLong }

/**
 * A playlist's card: its title, "12 items · 3h 12m" ([metaTexts]; [meta]
 * in English, the TV's until it shows them, and as it is in a card made
 * without them), whether it plays video or music, the Smart badge, and
 * the pictures of its first entries for the mosaic ([artwork], the
 * server's addresses, on [origin]).
 */
data class PlaylistCard(
    val id: Long,
    val title: String,
    val meta: String,
    val video: Boolean,
    val smart: Boolean,
    val artwork: List<String>,
    val origin: String,
    val metaTexts: List<UiText> = listOfNotNull(meta.takeIf { it.isNotEmpty() }?.let(UiText::raw)),
)

/**
 * Work started from a page or sheet that may close before it ends: a
 * change to send, a playlist to make. It runs to its end on the main
 * thread's loop, whatever happens to the screen.
 */
internal val PlaylistWork: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
