package app.pley.android.model

import kotlinx.serialization.Serializable

/** The order of a collection's items. */
@Serializable(with = CollectionSort.Serializer::class)
enum class CollectionSort(override val wire: String) : WireEnum {
    Release("release"),
    Title("title"),

    /** The order an administrator gave. */
    Custom("custom"),
    Unknown("unknown"),
    ;

    object Serializer : LenientEnumSerializer<CollectionSort>("CollectionSort", CollectionSort.entries, CollectionSort.Unknown)
}

/**
 * A collection: movies or shows of one library, shared by every profile
 * (apps/server/src/collections.rs, `CollectionDto`). Times are unix seconds.
 */
@Serializable
data class CollectionSummary(
    val id: Long,
    val libraryId: Long,
    val title: String,
    val summary: String? = null,
    /** Mirrors a TMDB movie collection. */
    val automatic: Boolean = false,
    val sort: CollectionSort = CollectionSort.Unknown,
    /** Smart collections: the library filter that fills them (the Plex-compatible API's syntax). */
    val smartFilter: String? = null,
    /** Its category: Pley's Other (`key` "other") when it has none; `null` from a server older than categories. */
    val category: CategoryRef? = null,
    /** Items this profile can see. */
    val itemCount: Int = 0,
    /** Its own artwork, else its first item's that has some. */
    val posterUrl: String? = null,
    val backdropUrl: String? = null,
    val createdAt: Long = 0,
    val updatedAt: Long = 0,
)

/** GET /api/collections/{id}: a collection and its items, in its order. */
@Serializable
data class CollectionDetail(
    val id: Long,
    val libraryId: Long,
    val title: String,
    val summary: String? = null,
    val automatic: Boolean = false,
    val sort: CollectionSort = CollectionSort.Unknown,
    val smartFilter: String? = null,
    val category: CategoryRef? = null,
    val itemCount: Int = 0,
    val posterUrl: String? = null,
    val backdropUrl: String? = null,
    val createdAt: Long = 0,
    val updatedAt: Long = 0,
    val items: List<ItemSummary> = emptyList(),
) {
    fun toSummary(): CollectionSummary = CollectionSummary(
        id = id,
        libraryId = libraryId,
        title = title,
        summary = summary,
        automatic = automatic,
        sort = sort,
        smartFilter = smartFilter,
        category = category,
        itemCount = itemCount,
        posterUrl = posterUrl,
        backdropUrl = backdropUrl,
        createdAt = createdAt,
        updatedAt = updatedAt,
    )
}

/** `{ collections }`: GET /api/libraries/{id}/collections, by title. */
@Serializable
data class CollectionList(
    val collections: List<CollectionSummary> = emptyList(),
)

/** A collection named only: what an item is "Part of". */
@Serializable
data class CollectionRef(
    val id: Long,
    val title: String,
)

/** `{ collections }`: GET /api/items/{id}/collections, those the movie or show is in. */
@Serializable
data class ItemCollections(
    val collections: List<CollectionRef> = emptyList(),
)

/** A collection's category as the collection names it: Pley's own ([key]) in the user's language, or an administrator's. */
@Serializable
data class CategoryRef(
    val id: Long,
    /** Pley's own ("highlights", "box_sets"); `null`: an administrator's. */
    val key: String? = null,
    val title: String,
)

/**
 * A category of collections (apps/server, `CategoryDto`): shared by every
 * library, in the order administrators chose.
 */
@Serializable
data class CollectionCategory(
    val id: Long,
    val key: String? = null,
    val title: String,
    /** The group label it was made from (Kometa's "Seasonal"). */
    val fromLabel: String? = null,
    val position: Int = 0,
)

/** `{ categories }`: GET /api/collection-categories, in order. */
@Serializable
data class CollectionCategories(
    val categories: List<CollectionCategory> = emptyList(),
)

/** One of a merged collection's collections, and the library it is in. */
@Serializable
data class MergedPart(
    val id: Long,
    val libraryId: Long,
)

/**
 * GET /api/collections/merged?ids=: a Home row of several libraries'
 * collections of one title, every one of its titles in the row's order.
 */
@Serializable
data class MergedCollection(
    val title: String,
    val collections: List<MergedPart> = emptyList(),
    val items: List<ItemSummary> = emptyList(),
)
