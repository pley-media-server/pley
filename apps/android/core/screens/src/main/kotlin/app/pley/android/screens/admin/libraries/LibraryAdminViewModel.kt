package app.pley.android.screens.admin.libraries

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.data.ServerAdmin
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.PleyJson
import app.pley.android.model.admin.LibraryChange
import app.pley.android.model.admin.LibraryScan
import app.pley.android.model.admin.NewLibrary
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.AdminLoad
import app.pley.android.screens.admin.AdminViewModel
import app.pley.android.screens.admin.folders.FolderBrowser
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch

/** A library's page: the library and its scan; for a new one, neither. */
data class LibraryPage(val library: Library?, val scan: LibraryScan?) {
    /** A scan of it is queued or running. */
    val scanning: Boolean get() = library?.scanning == true || scan?.running == true
}

/** What the library form holds: what is typed and picked, not saved yet. */
data class LibraryForm(
    val name: String = "",
    val kind: LibraryKind = LibraryKind.Movies,
    val paths: List<String> = emptyList(),
    /** Saving was tried without a folder: said before anything is sent, as the web does. */
    val noFolder: Boolean = false,
)

/**
 * A library's page of Settings → Libraries, or a new one's
 * (apps/web/src/pages/settings/LibraryFormModal.tsx): its kind (for a new
 * one), name and folders, picked on the server ([folders]) or typed, sent
 * together; its scans; and deleting it. [libraryId] is `null` for a new one.
 */
class LibraryAdminViewModel(admin: ServerAdmin, private val libraryId: Long?) : AdminViewModel(admin) {
    val page = resource {
        val id = libraryId ?: return@resource LibraryPage(null, null)
        coroutineScope {
            val library = async { get(Library.serializer(), listOf("api", "libraries", id)) }
            val scans = async { readScans() }
            LibraryPage(library.await(), LibraryAdmin.scanOf(id, scans.await()))
        }
    }

    private val formNow = MutableStateFlow(if (libraryId == null) LibraryForm() else null)

    /** The form, from the library as first read; `null` until then. Reading it again later leaves what is typed alone. */
    val form: StateFlow<LibraryForm?> = formNow.asStateFlow()

    /** Picking a folder on the server. */
    val folders = FolderBrowser(viewModelScope, admin::api)

    private val done = Channel<Unit>(Channel.CONFLATED)

    /** The library was made, saved or deleted: the page closes. */
    val finished: Flow<Unit> = done.receiveAsFlow()

    val isNew: Boolean get() = libraryId == null

    init {
        if (libraryId != null) {
            viewModelScope.launch {
                val first = page.state.first { it is AdminLoad.Ready } as AdminLoad.Ready
                val library = first.value.library ?: return@launch
                if (formNow.value == null) formNow.value = LibraryForm(library.name, library.kind, library.paths)
            }
        }
    }

    fun setName(name: String) = edit { it.copy(name = name) }

    /** A new library's kind; a library's stays what it was made as. */
    fun setKind(kind: LibraryKind) {
        if (isNew) edit { it.copy(kind = kind) }
    }

    /** Adds [path] (picked or typed), once. */
    fun addPath(path: String) {
        val trimmed = path.trim()
        if (trimmed.isEmpty()) return
        edit { if (trimmed in it.paths) it else it.copy(paths = it.paths + trimmed, noFolder = false) }
    }

    fun removePath(path: String) = edit { it.copy(paths = it.paths - path) }

    /** The form differs from the library as saved (a new one's always can be sent). */
    fun changed(form: LibraryForm, library: Library?): Boolean =
        library == null || form.name.trim() != library.name || form.paths != library.paths

    /**
     * Makes the library, or saves its name and folders; a name left empty
     * is the kind's [defaultName], as the web does. Without a folder,
     * nothing is sent and the form says so.
     */
    fun save(defaultName: String) {
        val form = formNow.value ?: return
        if (form.paths.isEmpty()) {
            formNow.value = form.copy(noFolder = true)
            return
        }
        val name = form.name.trim().ifEmpty { defaultName }
        var ok = false
        val close: () -> Unit = { if (ok) done.trySend(Unit) }
        if (libraryId == null) {
            change(said = AdminDone(LibraryAdmin.ADDED, listOf(name)), after = close) {
                val body = PleyJson.encodeToJsonElement(NewLibrary.serializer(), NewLibrary(name, form.kind, form.paths))
                send("POST", Library.serializer(), LibraryAdmin.LIBRARIES, body)
                ok = true
            }
        } else {
            change(said = AdminDone(LibraryAdmin.UPDATED), after = close) {
                val body = PleyJson.encodeToJsonElement(LibraryChange.serializer(), LibraryChange(name, form.paths))
                send("PATCH", Library.serializer(), listOf("api", "libraries", libraryId), body)
                ok = true
            }
        }
    }

    /** Scans the library; [force] probes every file again and refreshes the metadata. */
    fun scan(force: Boolean) {
        val library = page.state.value.valueOrNull?.library ?: return
        change(
            said = AdminDone(if (force) LibraryAdmin.RESCAN_STARTED else LibraryAdmin.SCAN_STARTED, listOf(library.name)),
            after = page::reload,
        ) { scanLibrary(library.id, force) }
    }

    /** Deletes the library (asked first): its items and watch history go, its files stay. */
    fun delete() {
        val library = page.state.value.valueOrNull?.library ?: return
        var ok = false
        val close: () -> Unit = { if (ok) done.trySend(Unit) }
        change(said = AdminDone(LibraryAdmin.DELETED, listOf(library.name)), after = close) {
            call("DELETE", listOf("api", "libraries", library.id))
            ok = true
        }
    }

    /** Reads the library and its scan again every few seconds while it is scanned; run it while the page is on screen. */
    suspend fun watchScans() {
        while (true) {
            delay(LibraryAdmin.SCAN_POLL_MS)
            if (page.state.value.valueOrNull?.scanning == true) page.reload()
        }
    }

    private fun edit(change: (LibraryForm) -> LibraryForm) {
        formNow.value = formNow.value?.let(change)
    }

    companion object {
        /** The page of the library [id] names; `null` or anything but a library's id: a new one. */
        fun factory(id: String?): ViewModelProvider.Factory = viewModelFactory {
            initializer { LibraryAdminViewModel(screensGraph().data.admin, id?.toLongOrNull()) }
        }
    }
}
