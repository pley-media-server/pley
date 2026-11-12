package app.pley.android.screens.admin.folders

import app.pley.android.model.PleyJson
import app.pley.android.model.admin.FolderListing
import app.pley.android.model.admin.FolderQuery
import app.pley.android.network.AdminApi
import app.pley.android.screens.admin.AdminLoad
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/** The folder picker, open on [path] (`null`: the server's drives and places to start from), and what it read there. */
data class FolderView(val path: String?, val load: AdminLoad<FolderListing>) {
    /** At the drives: nothing to go up to, nothing to choose. */
    val atRoots: Boolean get() = path == null

    /** The folder shown, once read: what Select chooses. */
    val chosen: String? get() = (load as? AdminLoad.Ready)?.value?.path
}

/**
 * Picking a folder on the server, as the web's FolderPicker does (POST
 * /api/fs): a library's, or the transcoder's. The page that picks owns
 * one, in its ViewModel's [scope], so the folder shown survives turning
 * the phone; [view] is `null` while the picker is closed.
 */
class FolderBrowser(private val scope: CoroutineScope, private val api: suspend () -> AdminApi) {
    private val now = MutableStateFlow<FolderView?>(null)
    private var job: Job? = null

    val view: StateFlow<FolderView?> = now.asStateFlow()

    /** Opens the picker on [path], or at the drives. */
    fun open(path: String?) = go(path)

    /** Shows the folders in [path] (typed, or tapped), or the drives for `null` or a blank one. */
    fun go(path: String?) {
        val asked = path?.trim()?.takeIf { it.isNotEmpty() }
        job?.cancel()
        now.value = FolderView(asked, AdminLoad.Loading)
        job = scope.launch {
            now.value = try {
                val body = PleyJson.encodeToJsonElement(FolderQuery.serializer(), FolderQuery(asked))
                FolderView(asked, AdminLoad.Ready(api().send("POST", FolderListing.serializer(), FS, body)))
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                FolderView(asked, AdminLoad.Failed(e))
            }
        }
    }

    /** The folder above the one shown; the drives from a drive's top, or from a folder that couldn't be read. */
    fun up() {
        val shown = now.value ?: return
        go((shown.load as? AdminLoad.Ready)?.value?.parent)
    }

    fun close() {
        job?.cancel()
        now.value = null
    }

    private companion object {
        val FS = listOf("api", "fs")
    }
}
