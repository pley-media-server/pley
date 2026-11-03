package app.pley.android.screens.drive

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.data.drive.Drive
import app.pley.android.data.drive.DriveEntry
import app.pley.android.data.drive.DriveFolder
import app.pley.android.data.drive.DriveSpace
import app.pley.android.data.drive.Drives
import app.pley.android.player.PlayRequest
import app.pley.android.screens.screensGraph
import java.io.IOException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** A line of a drive's folder: a folder, or a video and where it stopped. */
data class DriveLine(
    /** Where it is on the device; the line's key too. */
    val path: String,
    /** A folder's name; a video's, without its extension. */
    val name: String,
    val folder: Boolean,
    /** A video's kind, as its extension says it: "MKV". */
    val kind: String = "",
    /** Where a video resumes; 0 when it starts from the beginning. */
    val resumeMs: Long = 0,
    /** How far through it is, 2 % at least once started; 0 when its length isn't kept. */
    val progress: Float = 0f,
    /** A video's size in bytes; 0 when the drive won't say. */
    val sizeBytes: Long = 0,
)

/** Where the page is on the drive. */
sealed interface DrivePlace {
    /** The drive's top, with how full it is; `null` when the device won't say. */
    data class Top(val space: DriveSpace?) : DrivePlace

    /** A folder under it, by the names of the folders it is in, the drive's first: ["Films", "2019"]. */
    data class Under(val folders: List<String>) : DrivePlace
}

/** What the drive's page shows. */
sealed interface DrivePage {
    /** Asking the system for leave to read the drive, or reading the folder the first time. */
    data object Loading : DrivePage

    /** Pley may not read the drive: it was refused, and the device's settings for apps can allow it. */
    data object Refused : DrivePage

    /** No drive is plugged in, or not the one this folder is on. */
    data object NoDrive : DrivePage

    /** The folder can't be read: the drive may have been pulled out. [up]: a ".." line goes up a folder. */
    data class Unreadable(val place: DrivePlace, val up: Boolean) : DrivePage

    /** The folder's folders, then its videos; with none, there is nothing to play here. [up]: a ".." line first. */
    data class Folder(val place: DrivePlace, val up: Boolean, val lines: List<DriveLine>) : DrivePage
}

data class DriveUiState(
    val page: DrivePage = DrivePage.Loading,
    /** The drive's make and model, the page's title ([Drive.name]); `null` without a drive or a name. */
    val name: String? = null,
    /** Permissions to ask the system for now: the screen asks, then calls [DriveViewModel.asked]. */
    val ask: List<String> = emptyList(),
)

/**
 * A folder of the drive plugged into the TV ([folder]; `null`: the drive's
 * top), as the Samsung app's StorageScreen shows it: the drive's name,
 * where it is, its folders then its videos ([DriveFolder]), and each
 * video's line with where it stopped as this device keeps it. The folder
 * is read off the main thread, as a drive that sleeps is slow to answer;
 * again when a drive comes or goes; and its lines again each time the
 * page shows (back from the player). Leave to read the drive is asked for
 * as the page opens; refused, the page says so.
 */
class DriveViewModel(
    private val drives: Drives,
    val folder: String?,
    private val player: (PlayRequest) -> Unit,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : ViewModel() {
    private val _state = MutableStateFlow(DriveUiState())
    val state: StateFlow<DriveUiState> = _state.asStateFlow()

    /** The folder as it was last read, for its lines again. */
    private var entries: List<DriveEntry> = emptyList()
    private var watching = false

    init {
        val missing = drives.missing()
        if (missing.isEmpty()) watch() else _state.update { it.copy(ask = missing) }
    }

    /** The system's question is on screen. */
    fun asked() = _state.update { it.copy(ask = emptyList()) }

    /** The system's question was answered: the folder, or why it can't show. */
    fun permissionsAnswered() {
        if (drives.missing().isEmpty()) {
            // Before it was allowed, the drive's folder may not have shown as one.
            drives.refresh()
            watch()
        } else {
            _state.update { it.copy(page = DrivePage.Refused, ask = emptyList()) }
        }
    }

    /**
     * The page shows (again): allowed in the settings meanwhile, it reads the
     * folder; back from the player, each video's line says where it resumes
     * now. Drives are looked for again, for TV builds that don't say when
     * one comes or goes.
     */
    fun resumed() {
        if (!watching) {
            if (_state.value.page == DrivePage.Refused && drives.missing().isEmpty()) permissionsAnswered()
            return
        }
        drives.refresh()
        if (_state.value.page !is DrivePage.Folder) return
        val shown = entries
        viewModelScope.launch {
            val lines = withContext(io) { lines(shown) }
            // Read again meanwhile (a drive came or went): what was read stands.
            if (entries !== shown) return@launch
            _state.update { s -> (s.page as? DrivePage.Folder)?.let { s.copy(page = it.copy(lines = lines)) } ?: s }
        }
    }

    /** Plays a video in Pley's player, from where it stopped. */
    fun play(line: DriveLine) {
        if (!line.folder) player(PlayRequest.drive(line.path))
    }

    private fun watch() {
        if (watching) return
        watching = true
        viewModelScope.launch { drives.drive.collectLatest(::show) }
    }

    /** The folder on [drive]: what is there now, or why nothing is. */
    private suspend fun show(drive: Drive?) {
        val root = drive?.root?.trimEnd('/')?.ifEmpty { null }
        val dir = folder?.trimEnd('/') ?: root
        if (root == null || dir == null || !(dir == root || dir.startsWith("$root/"))) {
            entries = emptyList()
            _state.update { it.copy(page = DrivePage.NoDrive, name = null) }
            return
        }
        val up = dir != root
        val under = DrivePlace.Under(dir.removePrefix(root).split('/').filter { it.isNotEmpty() })
        val read = try {
            withContext(io) {
                val found = DriveFolder.entries(dir, drives.files.read(dir))
                Triple(found, lines(found), if (up) under else DrivePlace.Top(drives.files.space(root)))
            }
        } catch (_: IOException) {
            null
        } catch (_: SecurityException) {
            null
        }
        if (read == null) {
            entries = emptyList()
            _state.update { it.copy(page = DrivePage.Unreadable(if (up) under else DrivePlace.Top(null), up), name = drive.name) }
            return
        }
        val (found, lines, place) = read
        entries = found
        _state.update { it.copy(page = DrivePage.Folder(place, up, lines), name = drive.name) }
    }

    /** [found]'s lines: a video's with where it stopped, as this device keeps it (a read of its preferences). */
    private fun lines(found: List<DriveEntry>): List<DriveLine> = found.map { e ->
        if (e.folder) return@map DriveLine(e.path, e.name, folder = true)
        val kept = drives.resume.kept(e.path)
        val through = if (kept.positionMs > 0 && kept.durationMs > 0) kept.positionMs.toFloat() / kept.durationMs else 0f
        DriveLine(
            path = e.path,
            name = DriveFolder.title(e.path),
            folder = false,
            kind = DriveFolder.kind(e.name),
            resumeMs = kept.positionMs,
            progress = if (through > 0f) through.coerceIn(MIN_PROGRESS, 1f) else 0f,
            sizeBytes = e.size,
        )
    }

    companion object {
        /** A bar of a video started is never thinner than this, so it shows. */
        const val MIN_PROGRESS = 0.02f

        fun factory(folder: String?): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val g = screensGraph()
                DriveViewModel(g.drives, folder, g.playerLauncher::play)
            }
        }
    }
}
