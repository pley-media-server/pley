package app.pley.android.ui.phone.home

import androidx.compose.runtime.Immutable
import app.pley.android.common.AppLanguage
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.model.ItemSummary
import app.pley.android.screens.PleyKey
import app.pley.android.screens.home.HomeState
import app.pley.android.screens.home.RowShape
import app.pley.android.ui.phone.kit.HubRowModel

/**
 * Home as the phone draws it: [HomeState] with each row's cards made
 * ([ArtworkTileModel.of]), worked out off the main thread by the
 * ViewModel ([app.pley.android.screens.home.HomeViewModel.present]).
 */
@Immutable
internal data class PhoneHomeModel(
    val state: HomeState,
    val rows: List<PhoneHomeRow> = emptyList(),
) {
    companion object {
        fun of(state: HomeState): PhoneHomeModel {
            if (state !is HomeState.Ready) return PhoneHomeModel(state)
            val words = AppLanguage.formatWords()
            return PhoneHomeModel(
                state,
                state.rows.map { row ->
                    val kind = row.shape.tileKind
                    PhoneHomeRow(
                        model = HubRowModel(row.key, row.title, kind, row.items.map { ArtworkTileModel.of(it, kind, state.origin, words) }),
                        items = row.items,
                        seeAll = row.seeAll,
                    )
                },
            )
        }
    }
}

/** A row of cards, with the items they stand for, and where its title goes. */
@Immutable
internal data class PhoneHomeRow(
    val model: HubRowModel,
    val items: List<ItemSummary>,
    val seeAll: PleyKey?,
) {
    /** The item of [tile]: by its id, in this row. */
    fun itemOf(tile: ArtworkTileModel): ItemSummary? = items.firstOrNull { it.id == tile.id }
}

/** The card a row shape draws on phones. */
internal val RowShape.tileKind: TileKind
    get() = when (this) {
        RowShape.Landscape -> TileKind.Landscape
        RowShape.Poster -> TileKind.Poster
        RowShape.Square -> TileKind.Square
    }
