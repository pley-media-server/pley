package app.pley.android.designsystem.tile

import androidx.compose.runtime.Immutable
import app.pley.android.designsystem.image.ArtworkCandidates
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.cardSubtitle
import app.pley.android.model.format.episodeLabel
import app.pley.android.model.format.formatRuntime
import app.pley.android.model.format.formatTimeLeft
import app.pley.android.model.format.isInProgress
import app.pley.android.model.format.progressFraction
import app.pley.android.model.format.qualityBadges
import app.pley.android.model.format.tileDescription
import app.pley.android.model.format.unwatchedCount
import java.util.Locale

/**
 * Everything a tile shows, worked out once, off the main thread, when a
 * screen builds its state: the picture's addresses, the caption, the
 * overlays and what a screen reader says. Tiles only draw it.
 */
@Immutable
data class ArtworkTileModel(
    /** The item's id: the tile's key in lazy rows. */
    val id: Long,
    val title: String,
    /** The caption's second line: "2024", "3 seasons", "S2 · E5 · 26 min left". */
    val subtitle: String = "",
    val image: ArtworkImage? = null,
    /** Played so far, 0 to 1; 0 draws no bar. */
    val progress: Float = 0f,
    /** The watched tick. */
    val watched: Boolean = false,
    /** Shows and seasons: the unwatched count, drawn instead of the tick while above 0. */
    val unwatched: Int = 0,
    /** Short quality badges on the art ("4K", "DV P8.1", "Atmos"), bottom left. */
    val badges: List<String> = emptyList(),
    /** What a screen reader says: "Glasshouse, Season 2 Episode 5, 26 minutes left". */
    val description: String = title,
) {
    companion object {
        /**
         * The model of [item] drawn as [kind], its pictures on the server at
         * [origin]. Badges come from what a listing knows (its
         * [ItemSummary.media]) when [badges] is true.
         */
        fun of(
            item: ItemSummary,
            kind: TileKind,
            origin: String,
            badges: Boolean = false,
            locale: Locale = Locale.getDefault(),
        ): ArtworkTileModel = of(item, kind, origin, FormatWords.English, badges, locale)

        /**
         * [of] with its words in [words] (`formatWords()`, the language
         * shown), its counts in [locale]'s digits.
         */
        fun of(
            item: ItemSummary,
            kind: TileKind,
            origin: String,
            words: FormatWords,
            badges: Boolean = false,
            locale: Locale = words.locale,
        ): ArtworkTileModel {
            val inProgress = isInProgress(item)
            val timeLeft = if (inProgress) formatTimeLeft(item.userState.positionMs, item.durationMs, words) else ""
            val (title, subtitle) = when {
                kind == TileKind.Episode -> {
                    val numbered = if (item.index != null) "${item.index}. ${item.title}" else item.title
                    numbered to timeLeft.ifEmpty { formatRuntime(item.durationMs, words) }
                }
                kind == TileKind.Landscape && item.kind == ItemKind.Episode -> {
                    val show = item.grandparentTitle?.takeIf { it.isNotEmpty() } ?: item.title
                    show to listOf(episodeLabel(item, words), timeLeft.ifEmpty { item.title.takeIf { show != it }.orEmpty() })
                        .filter { it.isNotEmpty() }.joinToString(" · ")
                }
                else -> item.title to timeLeft.ifEmpty { cardSubtitle(item, words, locale) }
            }
            val image = when (kind) {
                TileKind.Landscape, TileKind.Episode -> ArtworkCandidates.thumb(item, origin)
                else -> ArtworkCandidates.poster(item, origin)
            }
            val unwatched = if (item.kind == ItemKind.Show || item.kind == ItemKind.Season) unwatchedCount(item) else 0
            return ArtworkTileModel(
                id = item.id,
                title = title,
                subtitle = subtitle,
                image = image,
                progress = if (inProgress) progressFraction(item) else 0f,
                watched = item.userState.watched && unwatched == 0,
                unwatched = unwatched,
                badges = if (badges) qualityBadges(item.media) else emptyList(),
                description = tileDescription(item, words, locale),
            )
        }
    }
}
