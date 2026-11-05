package app.pley.android.data

import app.pley.android.model.CollectionDetail
import app.pley.android.model.HistoryPage
import app.pley.android.model.Hub
import app.pley.android.model.HubKeys
import app.pley.android.model.Hubs
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemList
import app.pley.android.model.ItemSummary
import app.pley.android.model.ItemsPage
import app.pley.android.model.MaybeItem
import app.pley.android.model.PersonDetail
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.SearchResults

/**
 * Changes to items wherever they are shown: every list, row and page held
 * in memory that holds the item gets the change at once. Values that
 * don't change come back as the same instance, so screens showing them
 * don't redraw.
 */
internal class ItemPatch(
    private val summary: (ItemSummary) -> ItemSummary,
    private val detail: (ItemDetail) -> ItemDetail = { it },
    private val hub: (Hub) -> Hub = { it },
) {
    /** [value] changed, or `null` when nothing in it changed. */
    fun apply(value: Any): Any? = any(value).takeIf { it !== value }

    private fun any(value: Any): Any = when (value) {
        is ItemSummary -> summary(value)
        is ItemDetail -> detailOf(value)
        is Hubs -> {
            var changed = false
            val hubs = value.hubs.mapNotNull { h ->
                val items = items(h.items)
                val next = hub(if (items === h.items) h else h.copy(items = items))
                if (next !== h) changed = true
                next.takeIf { it.items.isNotEmpty() }
            }
            if (changed) Hubs(hubs) else value
        }
        is ItemList -> items(value.items).let { if (it === value.items) value else ItemList(it) }
        is ItemsPage -> items(value.items).let { if (it === value.items) value else value.copy(items = it) }
        is LibraryItems -> items(value.items).let { if (it === value.items) value else value.copy(items = it) }
        is MaybeItem -> value.item?.let(summary)?.let { if (it === value.item) value else MaybeItem(it) } ?: value
        is SearchResults -> items(value.items).let { if (it === value.items) value else value.copy(items = it) }
        is CollectionDetail -> items(value.items).let { if (it === value.items) value else value.copy(items = it) }
        is PlaylistDetail -> {
            var changed = false
            val entries = value.entries.map { e ->
                val item = summary(e.item)
                if (item !== e.item) {
                    changed = true
                    e.copy(item = item)
                } else {
                    e
                }
            }
            if (changed) value.copy(entries = entries) else value
        }
        is PersonDetail -> {
            var changed = false
            val credits = value.credits.map { c ->
                val item = summary(c.item)
                if (item !== c.item) {
                    changed = true
                    c.copy(item = item)
                } else {
                    c
                }
            }
            if (changed) value.copy(credits = credits) else value
        }
        is HistoryPage -> {
            var changed = false
            val entries = value.entries.map { e ->
                val item = summary(e.item)
                if (item !== e.item) {
                    changed = true
                    e.copy(item = item)
                } else {
                    e
                }
            }
            if (changed) value.copy(entries = entries) else value
        }
        else -> value
    }

    private fun detailOf(value: ItemDetail): ItemDetail {
        val changed = detail(value)
        val extras = items(changed.extras)
        return if (extras === changed.extras) changed else changed.copy(extras = extras)
    }

    private fun items(items: List<ItemSummary>): List<ItemSummary> {
        var changed = false
        val out = items.map { s -> summary(s).also { if (it !== s) changed = true } }
        return if (changed) out else items
    }

    companion object {
        /**
         * [itemId] marked [watched] by hand: it, and everything under it
         * (a show's seasons and episodes), watched or not, with no point to
         * resume; its season and show count one more or one fewer watched
         * episode ([target] is the item as it was, when known). Marked
         * watched, it leaves Continue Watching.
         */
        fun watched(itemId: Long, watched: Boolean, target: ItemSummary?): ItemPatch {
            val delta = when {
                target == null || target.kind != ItemKind.Episode -> 0
                target.userState.watched == watched -> 0
                watched -> 1
                else -> -1
            }
            val containers = if (delta == 0) emptySet() else setOfNotNull(target?.parentId, target?.grandparentId)
            fun under(s: ItemSummary) = s.id == itemId || s.parentId == itemId || s.grandparentId == itemId
            return ItemPatch(
                summary = { s ->
                    when {
                        under(s) -> markedSummary(s, watched)
                        s.id in containers -> countedSummary(s, delta)
                        else -> s
                    }
                },
                detail = { d ->
                    when {
                        under(d.toSummary()) -> d.copy(
                            userState = d.userState.copy(watched = watched, positionMs = 0),
                            viewedLeafCount = d.leafCount?.let { if (watched) it else 0 } ?: d.viewedLeafCount,
                        )
                        d.id in containers -> {
                            val counted = countedSummary(d.toSummary(), delta)
                            d.copy(userState = counted.userState, viewedLeafCount = counted.viewedLeafCount)
                        }
                        else -> d
                    }
                },
                hub = { h ->
                    if (watched && h.key.startsWith(CONTINUE_WATCHING) && h.items.any(::under)) h.copy(items = h.items.filterNot(::under)) else h
                },
            )
        }

        /**
         * [itemId] out of every Continue Watching; an episode takes its
         * whole show with it ([target] is the item as it was, when known).
         */
        fun hiddenFromContinueWatching(itemId: Long, target: ItemSummary?): ItemPatch {
            val show = target?.takeIf { it.kind == ItemKind.Episode }?.grandparentId
            fun gone(s: ItemSummary) = s.id == itemId || (show != null && (s.id == show || s.grandparentId == show))
            return ItemPatch(
                summary = { it },
                hub = { h -> if (h.continueWatching && h.items.any(::gone)) h.copy(items = h.items.filterNot(::gone)) else h },
            )
        }

        /** [itemId]'s page says whether it is in My List. */
        fun inWatchlist(itemId: Long, inList: Boolean): ItemPatch = ItemPatch(
            summary = { it },
            detail = { d -> if (d.id == itemId && d.inWatchlist != inList) d.copy(inWatchlist = inList) else d },
        )

        /** The profile's own [rating] of [itemId] (`null`: none). */
        fun rating(itemId: Long, rating: Double?): ItemPatch = ItemPatch(
            summary = { s -> if (s.id == itemId) s.copy(userState = s.userState.copy(rating = rating)) else s },
            detail = { d -> if (d.id == itemId) d.copy(userState = d.userState.copy(rating = rating)) else d },
        )

        /**
         * The version and tracks [itemId] starts with next time: on [fileId],
         * or its first file. [audioIndex] `null` keeps the audio it had, and
         * [automaticAudio] says no track is picked any more.
         */
        fun streams(itemId: Long, fileId: Long?, audioIndex: Int?, subtitleIndex: Int?, automaticAudio: Boolean = false): ItemPatch = ItemPatch(
            summary = { it },
            detail = { d ->
                if (d.id != itemId || d.files.isEmpty()) {
                    d
                } else {
                    val file = fileId ?: d.files.first().id
                    d.copy(
                        files = d.files.map { f ->
                            if (f.id != file) {
                                f
                            } else {
                                f.copy(
                                    selectedAudio = audioIndex?.takeUnless { automaticAudio } ?: f.selectedAudio,
                                    audioPicked = when {
                                        automaticAudio -> false
                                        audioIndex != null -> true
                                        else -> f.audioPicked
                                    },
                                    selectedSubtitle = subtitleIndex,
                                )
                            }
                        },
                    )
                }
            },
        )

        private fun markedSummary(s: ItemSummary, watched: Boolean): ItemSummary = s.copy(
            userState = s.userState.copy(watched = watched, positionMs = 0),
            viewedLeafCount = s.leafCount?.let { if (watched) it else 0 } ?: s.viewedLeafCount,
        )

        private fun countedSummary(s: ItemSummary, delta: Int): ItemSummary {
            val leaves = s.leafCount ?: return s
            val viewed = ((s.viewedLeafCount ?: 0) + delta).coerceIn(0, leaves)
            return s.copy(viewedLeafCount = viewed, userState = s.userState.copy(watched = viewed == leaves))
        }

        private const val CONTINUE_WATCHING = "continue_watching"

        /**
         * Home's [hubs] with [item] put first in My List, or taken out of it
         * ([item] `null`: [itemId] leaves). A new My List goes after
         * Continue Watching, called [title]. `null` when nothing changes.
         */
        fun myList(hubs: Hubs, itemId: Long, item: ItemSummary?, title: String = MY_LIST_TITLE): Hubs? {
            val at = hubs.hubs.indexOfFirst { it.key == HubKeys.MY_LIST }
            val list = hubs.hubs.getOrNull(at)
            if (item == null) {
                if (list == null || list.items.none { it.id == itemId }) return null
                val left = list.items.filterNot { it.id == itemId }
                return Hubs(if (left.isEmpty()) hubs.hubs - list else hubs.hubs.map { if (it === list) it.copy(items = left) else it })
            }
            if (list != null) {
                if (list.items.firstOrNull()?.id == itemId) return null
                return Hubs(hubs.hubs.map { if (it === list) it.copy(items = listOf(item) + it.items.filterNot { s -> s.id == itemId }) else it })
            }
            val after = hubs.hubs.indexOfLast { it.key == HubKeys.CONTINUE_WATCHING } + 1
            val hub = Hub(HubKeys.MY_LIST, title, null, listOf(item))
            return Hubs(hubs.hubs.take(after) + hub + hubs.hubs.drop(after))
        }

        /** What the server calls Home's My List in English; it says so itself, in the language asked, once asked again. */
        const val MY_LIST_TITLE = "My List"

        /** The first [itemId] found among [values], as a summary. */
        fun find(values: List<Any?>, itemId: Long): ItemSummary? {
            var found: ItemSummary? = null
            val look = ItemPatch(summary = { s ->
                if (found == null && s.id == itemId) found = s
                s
            }, detail = { d ->
                if (found == null && d.id == itemId) found = d.toSummary()
                d
            })
            for (value in values) {
                if (value != null) look.apply(value)
                if (found != null) break
            }
            return found
        }
    }
}
