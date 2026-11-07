package app.pley.android.screens.admin.libraries

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.data.ServerAdmin
import app.pley.android.model.Library
import app.pley.android.model.LibraryIds
import app.pley.android.model.PleyJson
import app.pley.android.model.admin.LibraryScan
import app.pley.android.model.admin.LibraryScanRequest
import app.pley.android.model.admin.LibraryScans
import app.pley.android.network.AdminApi
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.AdminViewModel
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.screens.playlists.Playlists
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.builtins.ListSerializer

/**
 * Settings → Libraries (apps/web/src/pages/settings/LibrariesSettings.tsx):
 * the libraries in the order everyone sees them in, with their scans;
 * scanning one or all, and moving them. A scan under way is read again
 * every few seconds while the page shows ([watchScans]), as the web polls.
 */
class LibrariesAdminViewModel(admin: ServerAdmin) : AdminViewModel(admin) {
    /** Every library, in the shared order. */
    val libraries = resource { get(ListSerializer(Library.serializer()), LibraryAdmin.LIBRARIES, listOf("order" to "shared")) }

    /** The scans the server has, queued, running or just done. */
    val scans = resource { readScans() }

    /** One move is saved at a time, in the order they were made. */
    private val saving = Mutex()

    /** Moves made and not saved yet: only the last one's answer is shown, and no other read replaces the order meanwhile. */
    private var moving = 0

    /** The list has shown before: showing again, it reads what a library's page (or anyone else) changed. */
    private var shown = false

    /** Scans [library]; [force] probes every file again and refreshes the metadata. */
    fun scan(library: Library, force: Boolean) = change(
        said = AdminDone(if (force) LibraryAdmin.RESCAN_STARTED else LibraryAdmin.SCAN_STARTED, listOf(library.name)),
        after = ::readAgain,
    ) { scanLibrary(library.id, force) }

    fun scanAll() = change(said = AdminDone(LibraryAdmin.SCANNING_ALL), after = ::readAgain) {
        call("POST", LibraryAdmin.LIBRARIES + "scan")
    }

    /**
     * Moves the library at [from] to [to]: shown at once, then saved as
     * the order everyone sees. The server's answer is shown once no later
     * move waits; a refused move puts back what the server has.
     */
    fun move(from: Int, to: Int) {
        val list = libraries.state.value.valueOrNull ?: return
        if (from !in list.indices || to !in list.indices || from == to) return
        val next = Playlists.moved(list, from, to)
        libraries.set(next)
        moving += 1
        var saved = false
        change(after = {
            moving -= 1
            if (!saved) libraries.reload()
        }) {
            val answer = saving.withLock {
                val ids = PleyJson.encodeToJsonElement(LibraryIds.serializer(), LibraryIds(next.map { it.id }))
                send("PUT", ListSerializer(Library.serializer()), LibraryAdmin.LIBRARIES + "order", ids)
            }
            saved = true
            if (moving == 1) libraries.set(answer)
        }
    }

    /** Moves the library [libraryId] [by] places (up when negative), as the web's arrows and a screen reader's actions do. */
    fun moveBy(libraryId: Long, by: Int) {
        val from = libraries.state.value.valueOrNull?.indexOfFirst { it.id == libraryId } ?: return
        if (from >= 0) move(from, from + by)
    }

    /**
     * Run while the page is on screen: the libraries and scans are read
     * again each time it shows after the first (a library added, changed
     * or deleted on its page, a scan started elsewhere), then the scans
     * every few seconds while one is under way.
     */
    suspend fun watchScans() {
        if (shown) readAgain()
        shown = true
        while (true) {
            delay(LibraryAdmin.SCAN_POLL_MS)
            if (scanning().isNotEmpty()) poll()
        }
    }

    /**
     * Reads the scans again; the libraries too when a scan ended (their
     * counts and scan times changed) or one the list has as scanning isn't
     * under way any more. A read that fails is tried at the next turn.
     */
    internal suspend fun poll() {
        val before = scanning()
        val now = try {
            admin().readScans()
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            return
        }
        scans.set(now)
        val running = now.filter { it.running }.mapTo(HashSet()) { it.libraryId }
        val list = libraries.state.value.valueOrNull.orEmpty()
        if (moving == 0 && ((before - running).isNotEmpty() || list.any { it.scanning && it.id !in running })) libraries.reload()
    }

    /** The libraries with a scan under way, as the list and the scans say. */
    private fun scanning(): Set<Long> = LibraryAdmin.scanning(
        libraries.state.value.valueOrNull.orEmpty(),
        scans.state.value.valueOrNull.orEmpty(),
    )

    private fun readAgain() {
        scans.reload()
        if (moving == 0) libraries.reload()
    }

    companion object {
        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer { LibrariesAdminViewModel(screensGraph().data.admin) }
        }
    }
}

/** What the pages of managing the libraries share: their routes, their confirmations, and when a scan runs. */
object LibraryAdmin {
    internal val LIBRARIES = listOf("api", "libraries")
    internal val ACTIVITY = listOf("api", "activity")

    /** How often a scan under way is read again, as the web does. */
    const val SCAN_POLL_MS = 4_000L

    // What a change confirms ([AdminDone.what]), with the library's name.
    const val SCAN_STARTED = "libraries.scanStarted"
    const val RESCAN_STARTED = "libraries.rescanStarted"
    const val SCANNING_ALL = "libraries.scanningAll"
    const val ADDED = "libraries.added"
    const val UPDATED = "libraries.updated"
    const val DELETED = "libraries.deleted"

    /** The libraries with a scan queued or running: as the list has them, or as the scans say. */
    fun scanning(libraries: List<Library>, scans: List<LibraryScan>): Set<Long> =
        libraries.filter { it.scanning }.mapTo(HashSet()) { it.id } + scans.filter { it.running }.map { it.libraryId }

    /** The scan of [libraryId] among [scans], if the server has one. */
    fun scanOf(libraryId: Long, scans: List<LibraryScan>): LibraryScan? = scans.firstOrNull { it.libraryId == libraryId }
}

internal suspend fun AdminApi.readScans(): List<LibraryScan> = get(LibraryScans.serializer(), LibraryAdmin.ACTIVITY).scans

internal suspend fun AdminApi.scanLibrary(id: Long, force: Boolean) {
    call("POST", listOf("api", "libraries", id, "scan"), PleyJson.encodeToJsonElement(LibraryScanRequest.serializer(), LibraryScanRequest(force)))
}
