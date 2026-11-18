package app.pley.android.screens.collection

import app.pley.android.model.CategoryRef
import app.pley.android.model.CollectionCategory
import app.pley.android.model.CollectionSummary
import app.pley.android.screens.library.CollectionTile

/** The Collections tab's Category filter: every one, or one category (Pley's Other among them). */
sealed interface CategoryChoice {
    data object All : CategoryChoice

    data class Of(val id: Long) : CategoryChoice
}

/**
 * Collections of one category. The server sends Other as the category of
 * those without one; [category] is `null` only from a server older than
 * categories, where every collection is in the one group, with no heading.
 */
data class CollectionGroup<T : Any>(
    val category: CategoryRef?,
    val collections: List<CollectionTile<T>>,
) {
    /** The key of its heading and of its row: the category's id. */
    val key: String get() = category?.id?.toString() ?: "none"

    /** The choice that picks it; `null` for a group without a category, which can't be chosen. */
    val choice: CategoryChoice? get() = category?.let { CategoryChoice.Of(it.id) }

    /** Whether it is the one [choice] picks. */
    fun isChosen(choice: CategoryChoice): Boolean = choice == CategoryChoice.All || choice == this.choice
}

/**
 * A library's collections as the Collections tab shows them (apps/web's
 * CollectionsTab): those of the [category] chosen, by category. [headed] says whether the groups have headings: not when only
 * one shows. [choices] are the categories that have collections, offered
 * in the Category filter when there is more than one, or one is chosen.
 */
data class CollectionsView<T : Any>(
    val groups: List<CollectionGroup<T>>,
    val choices: List<CollectionGroup<T>>,
    /** The category chosen: one that has no collections any more chooses every one. */
    val category: CategoryChoice,
    /** How many collections the library has, whatever the filter. */
    val total: Int,
) {
    /** How many collections show. */
    val count: Int get() = groups.sumOf { it.collections.size }

    val headed: Boolean get() = groups.size > 1

    val filtered: Boolean get() = category != CategoryChoice.All

    /** The Category filter is offered. */
    val categoryOffered: Boolean get() = choices.size > 1 || filtered

    companion object {
        /** [collections] of the category [chosen], grouped by [categories] in their order. */
        fun <T : Any> of(collections: List<CollectionTile<T>>, categories: List<CollectionCategory>, chosen: CategoryChoice): CollectionsView<T> {
            val grouped = groupByCategory(collections, categories)
            val choices = grouped.filter { it.choice != null }
            val category = chosen.takeIf { chosen == CategoryChoice.All || choices.any { it.choice == chosen } } ?: CategoryChoice.All
            val groups = grouped.filter { it.isChosen(category) }
            return CollectionsView(groups, choices, category, collections.size)
        }

        /** [collections] in one group, with no headings and nothing to choose. */
        fun <T : Any> ungrouped(collections: List<CollectionTile<T>>): CollectionsView<T> = CollectionsView(
            groups = listOf(CollectionGroup<T>(null, collections)).filter { it.collections.isNotEmpty() },
            choices = emptyList(),
            category = CategoryChoice.All,
            total = collections.size,
        )

        /**
         * [collections] by the category the server gives each (Other for
         * those without one), in the order of [categories]; empty groups
         * left out. A collection of a category not listed yet (made since)
         * comes after the rest, under its own category.
         */
        fun <T : Any> groupByCategory(collections: List<CollectionTile<T>>, categories: List<CollectionCategory>): List<CollectionGroup<T>> {
            val groups = LinkedHashMap<Long?, MutableList<CollectionTile<T>>>()
            val refs = HashMap<Long?, CategoryRef?>()
            categories.forEach { category ->
                groups[category.id] = mutableListOf()
                refs[category.id] = CategoryRef(category.id, category.key, category.title)
            }
            collections.forEach { c ->
                val id = c.category?.id
                groups.getOrPut(id) { mutableListOf() }.add(c)
                refs.getOrPut(id) { c.category }
            }
            return groups.filterValues { it.isNotEmpty() }.map { (id, list) -> CollectionGroup(refs[id], list) }
        }
    }
}

/** A collection's tile with the category the Collections tab groups it by. */
fun <T : Any> CollectionSummary.tile(tile: T): CollectionTile<T> = CollectionTile(
    id = id,
    count = itemCount,
    tile = tile,
    category = category,
)
