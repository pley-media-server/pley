package app.pley.android.player.queue

import app.pley.android.data.LocalMedia
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.episodeLabel
import app.pley.android.model.format.isPlayableVideo
import app.pley.android.network.PleyApi
import app.pley.android.player.PlayQueue
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.withTimeoutOrNull

/**
 * What plays after an item, fetched fresh so its watch state is today's
 * (apps/web/src/lib/videoQueue.ts, nextInQueue; fetchNext): the queue's
 * next video when playing through a playlist or collection, else the next
 * episode; movies and extras have none. An episode played from a download
 * goes on to the next one downloaded when the server doesn't answer
 * ([local]).
 */
class NextUp(private val api: PleyApi, private val local: LocalMedia = LocalMedia.None) {
    /** Whether [kind] played from [queue] can have something next, before asking. */
    fun hasNext(kind: ItemKind, queue: PlayQueue?): Boolean = queue != null || kind == ItemKind.Episode

    /**
     * What follows [itemId] (of [kind]); `null` at the end. Played
     * [fromDownload], an episode asks the server for [SERVER_WAIT_MS] at
     * most, then takes the next episode downloaded.
     */
    suspend fun after(itemId: Long, kind: ItemKind, queue: PlayQueue?, fromDownload: Boolean = false): ItemSummary? = when (queue) {
        is PlayQueue.Playlist -> nextIn(api.playlist(queue.playlistId).entries.map { it.item }, itemId)
        is PlayQueue.Collection -> nextIn(api.collection(queue.collectionId).items, itemId)
        null -> when {
            kind != ItemKind.Episode -> null
            fromDownload -> nextAway(itemId)
            else -> api.next(itemId).item
        }
    }

    /** The server's next episode when it answers in time, else the next one downloaded. */
    private suspend fun nextAway(itemId: Long): ItemSummary? {
        val answer = try {
            withTimeoutOrNull(SERVER_WAIT_MS) { api.next(itemId) }
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            null
        }
        return if (answer != null) answer.item else local.nextAfter(itemId)
    }

    companion object {
        /** How long a download's player waits for the server before going on with downloads. */
        const val SERVER_WAIT_MS = 3_000L

        /** The first video after [itemId] in [items]; `null` when it is the last, or isn't there. */
        fun nextIn(items: List<ItemSummary>, itemId: Long): ItemSummary? {
            val videos = items.filter { isPlayableVideo(it.kind) }
            val at = videos.indexOfFirst { it.id == itemId }
            return if (at < 0) null else videos.getOrNull(at + 1)
        }

        /** Where [next] starts: from the start when watched, else its resume point. */
        fun startOf(next: ItemSummary): Long = if (next.userState.watched) 0 else next.userState.positionMs

        /** "S1 · E2" for an episode, in [words]; `null` otherwise. */
        fun labelOf(next: ItemSummary, words: FormatWords): String? =
            if (next.kind == ItemKind.Episode) episodeLabel(next, words).ifEmpty { null } else null
    }
}
