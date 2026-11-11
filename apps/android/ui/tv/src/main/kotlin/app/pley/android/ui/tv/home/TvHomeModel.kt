package app.pley.android.ui.tv.home

import androidx.compose.runtime.Immutable
import app.pley.android.common.AppLanguage
import app.pley.android.common.Tier
import app.pley.android.common.UiText
import app.pley.android.designsystem.image.ArtworkCandidates
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.model.ItemSummary
import app.pley.android.screens.PleyKey
import app.pley.android.screens.home.HomeInfo
import app.pley.android.screens.home.HomeState
import app.pley.android.screens.home.RowShape
import app.pley.android.screens.home.homeInfo

/**
 * Home as the TV draws it: [HomeState] with each row's tiles made
 * ([ArtworkTileModel.of]) and the heroes' backdrops, worked out off the main
 * thread by the ViewModel ([app.pley.android.screens.home.HomeViewModel.present]).
 */
@Immutable
internal data class TvHomeModel(
    val state: HomeState,
    val rows: List<TvHomeRow> = emptyList(),
    /**
     * What the hero takes turns showing: the featured titles, or without
     * any, the one hero of before (the first of Continue Watching). Empty
     * with the banner turned off: Home opens on its rows.
     */
    val heroes: List<TvHero> = emptyList(),
    /** Without a hero, what the info block shows until focus comes to a tile: the first row's first. */
    val first: TvHero? = null,
) {
    /** The hero shown first. */
    val hero: TvHero? get() = heroes.firstOrNull()

    /** What the page shows at rest: the hero, else the first tile, else nothing (nothing to draw). */
    val atRest: TvHero? get() = hero ?: first

    /**
     * Where focus goes if tile [tileId] leaves row [rowKey], as a focus
     * key: the tile after it, else the one before, else the first of the
     * row below (or above) when the row goes with it.
     */
    fun standIn(rowKey: String, tileId: Long): String? {
        val at = rows.indexOfFirst { it.key == rowKey }
        val row = rows.getOrNull(at) ?: return null
        val i = row.tiles.indexOfFirst { it.id == tileId }
        val beside = row.tiles.getOrNull(i + 1) ?: row.tiles.getOrNull(i - 1)?.takeIf { i > 0 }
        if (beside != null) return "$rowKey/${beside.id}"
        val other = rows.getOrNull(at + 1) ?: rows.getOrNull(at - 1) ?: return null
        return other.tiles.firstOrNull()?.let { "${other.key}/${it.id}" }
    }

    /**
     * Whether focus key [key] ("<row key>/<tile id>", as the rows remember
     * focus) is one of these rows' tiles. Looked up when asked, rather than
     * a set of every tile's key made for each new answer.
     */
    fun hasTile(key: String): Boolean {
        val slash = key.lastIndexOf('/')
        if (slash < 0) return false
        val id = key.substring(slash + 1).toLongOrNull() ?: return false
        val rowKey = key.substring(0, slash)
        return rows.any { row -> row.key == rowKey && row.tiles.any { it.id == id } }
    }

    companion object {
        /** The shaping for a device of [tier] (its backdrops' width). One per tier, so the ViewModel keeps one flow. */
        fun shaping(tier: Tier): (HomeState) -> TvHomeModel = SHAPINGS.getValue(tier)

        private val SHAPINGS: Map<Tier, (HomeState) -> TvHomeModel> = Tier.entries.associateWith { tier ->
            { state: HomeState -> of(state, tier) }
        }

        fun of(state: HomeState, tier: Tier): TvHomeModel {
            if (state !is HomeState.Ready) return TvHomeModel(state)
            val words = AppLanguage.formatWords()
            val rows = state.rows.map { row ->
                val kind = row.shape.tileKind
                TvHomeRow(
                    key = row.key,
                    title = row.title,
                    kind = kind,
                    tiles = row.items.map { ArtworkTileModel.of(it, kind, state.origin, words) },
                    items = row.items,
                    seeAll = row.seeAll,
                )
            }
            val featured = state.featured.map { TvHero(it.info, backdropOf(it.info.item, state.origin, tier), it.label) }
            val heroes = featured.ifEmpty { listOfNotNull(state.hero?.let { TvHero(it, backdropOf(it.item, state.origin, tier)) }) }
            val first = if (state.banner) null else state.rows.firstOrNull()?.items?.firstOrNull()?.let { TvHero(homeInfo(it, null), backdropOf(it, state.origin, tier)) }
            return TvHomeModel(state, rows, heroes, first)
        }

        /** The backdrop behind [item]: its own, else its show's or parent's, at most 1280 wide (960 on T0). */
        fun backdropOf(item: ItemSummary, origin: String, tier: Tier): ArtworkImage =
            ArtworkCandidates.backdrop(item, origin, tier)
    }
}

/** A row of tiles, with the items they stand for (by position). */
@Immutable
internal data class TvHomeRow(
    val key: String,
    val title: String,
    val kind: TileKind,
    val tiles: List<ArtworkTileModel>,
    val items: List<ItemSummary>,
    val seeAll: PleyKey?,
) {
    /** The item of [tile]: by its id, in this row. */
    fun itemOf(tile: ArtworkTileModel): ItemSummary? = items.firstOrNull { it.id == tile.id }

    /** The [count] items after [item] in the row, whose details are asked for ahead. */
    fun after(item: ItemSummary, count: Int = 2): List<ItemSummary> {
        val at = items.indexOfFirst { it.id == item.id }
        return if (at < 0) emptyList() else items.drop(at + 1).take(count)
    }

    /** The items either side of [item] in the row, the next one first. */
    fun beside(item: ItemSummary): List<ItemSummary> {
        val at = items.indexOfFirst { it.id == item.id }
        return if (at < 0) emptyList() else listOfNotNull(items.getOrNull(at + 1), items.getOrNull(at - 1))
    }
}

/** The hero: what it says, its backdrop, and for a featured title why it is there ("New in Movies"). */
@Immutable
internal data class TvHero(val info: HomeInfo, val backdrop: ArtworkImage, val label: UiText? = null)

/** The tile a row shape draws on TV. */
internal val RowShape.tileKind: TileKind
    get() = when (this) {
        RowShape.Landscape -> TileKind.Landscape
        RowShape.Poster -> TileKind.Poster
        RowShape.Square -> TileKind.Square
    }
