package app.pley.android.network

import app.pley.android.model.ServerEvent
import kotlinx.coroutines.flow.Flow

/** What the server's event stream brings. */
sealed interface LiveEvent {
    data class Event(val event: ServerEvent) : LiveEvent

    /**
     * Connected, the first time or again after a break. The server replays
     * nothing, so whatever was shown may be stale: refresh it.
     */
    data object Reconnected : LiveEvent

    /**
     * The connection was lost, and is tried again: until [Reconnected],
     * nothing is heard (what polls the server while it can't hear it,
     * requests' summary, polls now).
     */
    data object Dropped : LiveEvent
}

/**
 * The server's events (GET /api/events, server-sent events): library
 * changes and scans, while collected. It reconnects by itself, waiting
 * longer after each failure (2 s doubling to 30 s), and with a fresh token
 * when the stream ends with the old one. It ends with an [ApiException]
 * when the server refuses the app (401 signed out, 403 not allowed).
 */
fun interface ServerEvents {
    fun events(): Flow<LiveEvent>
}
