package app.pley.android.watchnext

import android.net.Uri
import android.util.Log
import app.pley.android.data.DataGraph
import app.pley.android.data.Loadable
import app.pley.android.data.SessionState
import app.pley.android.model.Hubs
import app.pley.android.network.NetworkGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.withContext

/**
 * Android TV's and Google TV's Watch Next row, kept to the profile signed
 * in: its Continue Watching from home's rows, which the app keeps current
 * already (progress, a title finished or taken off Continue Watching, the
 * server's events), so this asks the server for nothing of its own. Another
 * profile or server signed in replaces the row's programs; signing out
 * empties it. Runs while the TV app is in the foreground ([follow]); the
 * row keeps what it last showed meanwhile. Not on phones, and not on Fire
 * TV, whose home screen has no Watch Next row.
 */
class WatchNext(
    /** Made off the main thread, the first time the row is followed: the app's start never waits on them. */
    private val data: () -> DataGraph,
    private val network: () -> NetworkGraph,
    private val row: WatchNextRow,
    private val images: WatchNextImages,
    private val io: CoroutineDispatcher = Dispatchers.IO,
    private val now: () -> Long = System::currentTimeMillis,
) {
    /** What the row should show, as the session and home's rows change: `null` for nothing (signed out). */
    @OptIn(ExperimentalCoroutinesApi::class)
    private fun wanted(data: DataGraph): Flow<List<WatchNextEntry>> = data.session.state
        .map { state ->
            when (state) {
                SessionState.Starting -> Wait
                is SessionState.SignedIn -> state.session.me?.id?.let { WatchNextOwner(state.session.issuer, it) } ?: Wait
                else -> Nobody
            }
        }
        .distinctUntilChanged()
        .flatMapLatest { owner ->
            when (owner) {
                Wait -> emptyFlow()
                Nobody -> flowOf(emptyList())
                is WatchNextOwner -> data.hubs.home().mapNotNull { hubsOf(it) }.map { WatchNextPlan.of(it, owner, now()) }
                else -> emptyFlow()
            }
        }
        .distinctUntilChanged()

    /** Keeps the row to the profile's Continue Watching until canceled. */
    suspend fun follow() {
        val graph = withContext(io) { data() }
        wanted(graph).conflate().collectLatest { entries ->
            withContext(io) { sync(entries) }
        }
    }

    /** Makes the row show [entries]: pictures fetched first, then the programs changed. */
    internal fun sync(entries: List<WatchNextEntry>) {
        try {
            val published = row.published()
            val changes = WatchNextDiff.of(published, entries)
            Log.i(TAG, "${published.size} programs in the row, ${entries.size} wanted: ${changes.insert.size} added, ${changes.update.size} updated, ${changes.delete.size} removed")
            changes.delete.forEach(row::delete)
            val owner = entries.firstOrNull()?.owner
            val api = owner?.let { network().api(it.issuer, it.profileId) }
            val urls = HashMap<Long, String>()
            fun image(entry: WatchNextEntry): Uri? {
                val url = entry.imagePath?.let { api?.url(it, IMAGE_WIDTH) } ?: return null
                urls[entry.itemId] = url
                return images.uriFor(url)
            }
            changes.update.forEach { (id, entry) -> row.update(id, entry, image(entry)) }
            changes.insert.forEach { entry -> row.insert(entry, image(entry)) }
            images.keepOnly(urls.values)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            // No TV provider here (some Android TV boxes), or it refused: the row stays as it was.
            Log.w(TAG, "Watch Next not updated: $e")
        }
    }

    private fun hubsOf(state: Loadable<Hubs>): Hubs? = when (state) {
        is Loadable.Ready -> state.value
        is Loadable.Failed -> state.stale
        Loadable.Loading -> null
    }

    /** Not known yet who is signed in. */
    private object Wait

    /** Nobody signed in: the row empties. */
    private object Nobody

    private companion object {
        const val TAG = "pley.watchnext"

        /** The row's pictures are about this wide on a 1080p home screen. */
        const val IMAGE_WIDTH = 640
    }
}
