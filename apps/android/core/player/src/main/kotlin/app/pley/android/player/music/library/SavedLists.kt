package app.pley.android.player.music.library

import androidx.annotation.OptIn
import androidx.media3.common.MediaItem
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.MediaConstants
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/**
 * Lists a car was shown from what was saved, the server slow
 * ([freshOrSaved]), read again once the server has answered: when they
 * turn out different (other rows, or rows named, described or pictured
 * otherwise), the cars are told to read the list again ([changed]), and
 * then get the server's at once. Never when they are the same, so a list
 * doesn't flicker as it is read again for nothing.
 *
 * The pages of one list that differ within [settleMs] of each other are
 * told once. Main thread only.
 */
@OptIn(markerClass = [UnstableApi::class])
internal class SavedLists(
    private val scope: CoroutineScope,
    /** A page of a list, as a car is given it ([MusicLibrary.children]). */
    private val read: suspend (parentId: String, page: Int, pageSize: Int) -> List<MediaItem>,
    private val freshMs: Long = FRESH_MS,
    private val settleMs: Long = SETTLE_MS,
    /** Tells the cars that the list [parentId] changed. */
    private val changed: (parentId: String) -> Unit,
) {
    /** The page of each list waiting for the server's answer, the latest of each: "album:50#0". */
    private val waiting = HashMap<String, Job>()

    /** The lists found changed, told once [settleMs] have passed. */
    private val telling = HashMap<String, Job>()

    /** A car was given [shown], page [page] of [parentId]; [saved] says what of it was saved. */
    fun shown(parentId: String, page: Int, pageSize: Int, shown: List<MediaItem>, saved: SavedAnswers) {
        if (!saved.any) return
        val key = "$parentId#$page"
        waiting.remove(key)?.cancel()
        waiting[key] = scope.launch {
            try {
                // Not in: the server out of reach, or failing. What the car shows stays.
                if (withTimeoutOrNull(freshMs) { saved.fresh() } != true) return@launch
                val fresh = try {
                    read(parentId, page, pageSize)
                } catch (e: CancellationException) {
                    throw e
                } catch (_: Exception) {
                    return@launch
                }
                if (fresh.map(::seen) != shown.map(::seen)) tell(parentId)
            } finally {
                if (waiting[key] === coroutineContext[Job]) waiting.remove(key)
            }
        }
    }

    private fun tell(parentId: String) {
        if (telling[parentId]?.isActive == true) return
        telling[parentId] = scope.launch {
            delay(settleMs)
            telling.remove(parentId)
            changed(parentId)
        }
    }

    /** What a car shows of a row: what it is, its title, the line under it, its picture and the heading it is under. */
    private fun seen(item: MediaItem): List<Any?> {
        val m = item.mediaMetadata
        return listOf(
            item.mediaId,
            m.title?.toString(),
            m.displayTitle?.toString(),
            m.subtitle?.toString(),
            m.artworkUri,
            m.extras?.getString(MediaConstants.EXTRAS_KEY_CONTENT_STYLE_GROUP_TITLE),
        )
    }

    private companion object {
        /** The longest the server's answer is waited for: past that, the car has long moved on. */
        const val FRESH_MS = 30_000L

        /** Changes to one list close together (its pages, read again one after another) are told once. */
        const val SETTLE_MS = 500L
    }
}
