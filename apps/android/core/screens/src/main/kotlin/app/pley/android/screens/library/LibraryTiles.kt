package app.pley.android.screens.library

import app.pley.android.data.LibraryItems
import app.pley.android.model.CollectionSummary
import app.pley.android.model.ItemSummary
import app.pley.android.model.LibraryKind
import java.util.IdentityHashMap

/** How an item is drawn in a grid or row: the interfaces' tile kinds, as far as a screen needs to say. */
enum class TileLook {
    /** Movies and shows, 2:3, with no quality badges whatever the item carries. */
    Poster,

    /** Continue Watching, 16:9 stills. */
    Landscape,

    /** Music, 1:1. */
    Square,

    /** A poster with its quality badges ("4K", "DV"): a library grid filtered by resolution or dynamic range. */
    Badged,

    /** A poster that says what the files are like ("4K", "DV"; "42 Mbps · 23.4 GB"): sorted by bitrate. */
    Bitrate,
}

/**
 * Makes the tiles a screen shows, off the main thread: each interface has
 * its own tile model (the design system's), with its strings
 * ("12 movies"). [origin] is the server's address, for the pictures.
 */
interface TileFactory<T : Any> {
    fun item(item: ItemSummary, look: TileLook, origin: String): T

    /** A collection of a library of [kind]: its poster, its count, "12 movies". */
    fun collection(collection: CollectionSummary, kind: LibraryKind?, origin: String): T
}

/**
 * A grid's cells, however many the listing has ([total]): the tiles loaded
 * so far start at position [offset]; the others are `null` until the
 * listing's window gets there, and screens draw skeletons for them.
 */
class GridCells<T : Any>(
    val total: Int,
    val offset: Int,
    val tiles: List<T>,
    private val ids: LongArray,
) {
    /** The tile at [position]; `null` while it isn't loaded. */
    operator fun get(position: Int): T? = tiles.getOrNull(position - offset)

    /** The item id at [position], or -1. */
    fun idAt(position: Int): Long = ids.getOrElse(position - offset) { -1L }

    /**
     * A stable key for the cell at [position]: its item's id, or a
     * negative one for a cell still loading (ids are positive).
     */
    fun key(position: Int): Long = idAt(position).takeIf { it >= 0 } ?: -(position + 1L)

    /** Where [itemId] is in the listing, if loaded. */
    fun positionOf(itemId: Long): Int {
        val at = ids.indexOf(itemId)
        return if (at < 0) -1 else at + offset
    }

    /** Every cell is loaded. */
    val complete: Boolean get() = offset == 0 && tiles.size >= total

    override fun equals(other: Any?): Boolean =
        other is GridCells<*> && other.total == total && other.offset == offset && other.ids.contentEquals(ids) && other.tiles == tiles

    override fun hashCode(): Int = 31 * (31 * total + offset) + ids.contentHashCode()

    companion object {
        fun <T : Any> empty(): GridCells<T> = GridCells(0, 0, emptyList(), LongArray(0))
    }
}

/**
 * Builds a listing's tiles, keeping those of items it built before: a
 * listing that grows (its pages arriving) or moves its window rebuilds
 * only what is new. Items are compared by identity: the data side keeps
 * the same objects across pages and makes new ones when an item changes.
 */
internal class GridBuilder<T : Any>(private val factory: TileFactory<T>) {
    private var built = IdentityHashMap<ItemSummary, T>()
    private var builtFor: Pair<String, TileLook>? = null

    fun build(items: LibraryItems, look: TileLook, origin: String): GridCells<T> {
        if (builtFor != origin to look) {
            built = IdentityHashMap()
            builtFor = origin to look
        }
        val next = IdentityHashMap<ItemSummary, T>(items.items.size * 2)
        val tiles = ArrayList<T>(items.items.size)
        val ids = LongArray(items.items.size)
        items.items.forEachIndexed { i, item ->
            val tile = built[item] ?: factory.item(item, look, origin)
            next[item] = tile
            tiles += tile
            ids[i] = item.id
        }
        built = next
        return GridCells(maxOf(items.total, items.offset + items.items.size), items.offset, tiles, ids)
    }
}
