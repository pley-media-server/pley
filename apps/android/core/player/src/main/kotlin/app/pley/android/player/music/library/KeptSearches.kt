package app.pley.android.player.music.library

import androidx.media3.common.MediaItem

/**
 * The results of the last few searches a car sent ([most]), by query, for
 * [maxAgeMs]. A car searches as letters are typed, and asks for a search's
 * results only after saying it searched: a search overtaken by the next
 * letter, then asked again (a letter taken back), or whose results are
 * asked for after the next search, is answered from here rather than asked
 * of the server again. Safe from any thread.
 */
internal class KeptSearches(
    private val most: Int = MOST,
    private val maxAgeMs: Long = MAX_AGE_MS,
    private val clock: () -> Long = { System.nanoTime() / 1_000_000 },
) {
    /** By query, the one used last at the end. */
    private val kept = LinkedHashMap<String, Pair<Long, List<MediaItem>>>(most + 1, 0.75f, true)

    /** [query]'s results, when they were kept and are still new enough. */
    @Synchronized
    fun get(query: String): List<MediaItem>? {
        val (at, results) = kept[query] ?: return null
        if (clock() - at < maxAgeMs) return results
        kept.remove(query)
        return null
    }

    @Synchronized
    fun put(query: String, results: List<MediaItem>) {
        kept[query] = clock() to results
        while (kept.size > most) kept.remove(kept.keys.first())
    }

    /** Forgets every search: another profile's aren't this one's. */
    @Synchronized
    fun clear() = kept.clear()

    private companion object {
        /** The searches kept: the one typed, and the two before it. */
        const val MOST = 3

        /** How long a search's results are kept: the library may change, and the server is asked again after. */
        const val MAX_AGE_MS = 60_000L
    }
}
