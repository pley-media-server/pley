package app.pley.android.data

import app.pley.android.model.ServerEvent
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.emptyFlow

/** A library scan the server is running. */
data class ScanState(
    val libraryId: Long?,
    /** From 0 to 1 when the server says how far it is. */
    val progress: Float?,
)

/**
 * Keeps what the screens show in step with the server while the app is in
 * the foreground and signed in: a changed library refreshes what shows it,
 * and coming back refreshes what is old.
 */
interface LiveUpdates {
    /**
     * Scans running now; empty when none. Reading them [start]s following
     * the server: the shells and Home read them after the first frame.
     */
    val scans: StateFlow<List<ScanState>>

    /**
     * Starts following the server, once, after the first frame: from then
     * on the event stream is open whenever the app is in the foreground and
     * signed in, and closed otherwise. Calling it again does nothing. A
     * stream that comes back after failing (a server that wasn't listening
     * yet) asks again for what failed.
     */
    fun start() {}

    /**
     * Every event heard, as it comes, while the stream is open (the player
     * listens for its session being stopped). Nothing is replayed.
     */
    val events: Flow<ServerEvent> get() = emptyFlow()

    /**
     * Each time the event stream connects, the first time or again after a
     * break: the server answers now (the shells lower their offline notices).
     */
    val answered: Flow<Unit> get() = emptyFlow()

    /**
     * The event stream is open and connected now: false while it is closed
     * (in the background, signed out, refused) or lost and trying again.
     * What can't wait for events polls while it is false.
     */
    val streaming: StateFlow<Boolean> get() = NotStreaming
}

private val NotStreaming: StateFlow<Boolean> = MutableStateFlow(false).asStateFlow()
