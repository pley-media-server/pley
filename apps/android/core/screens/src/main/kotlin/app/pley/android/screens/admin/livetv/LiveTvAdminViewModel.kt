package app.pley.android.screens.admin.livetv

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.data.ServerAdmin
import app.pley.android.model.admin.LineupGroup
import app.pley.android.model.admin.TunerStatus
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.AdminViewModel
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.builtins.ListSerializer

/**
 * Live TV's first page, as the web's Sources tab without the sources,
 * which stay in the web app: the lineup's groups, which say whether
 * there are channels to manage, and the connections in use, which are
 * read again every few seconds while the page shows ([watch]), as the
 * web's are.
 */
class LiveTvAdminViewModel(admin: ServerAdmin) : AdminViewModel(admin) {
    val groups = resource { get(ListSerializer(LineupGroup.serializer()), listOf("api", "live", "lineup", "groups")) }

    private val tunersNow = MutableStateFlow<List<TunerStatus>?>(null)

    /** The connections in use; `null` until first read. */
    val tuners: StateFlow<List<TunerStatus>?> = tunersNow.asStateFlow()

    /**
     * Reads the connections in use every [TUNERS_EVERY] ms until cancelled
     * (the page goes off screen). A read that fails keeps what was shown;
     * the next one tries again.
     */
    suspend fun watch() {
        while (true) {
            readTuners()
            delay(TUNERS_EVERY)
        }
    }

    private suspend fun readTuners() {
        try {
            tunersNow.value = admin.api().get(ListSerializer(TunerStatus.serializer()), listOf("api", "live", "tuners"))
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            // The next read tries again; what was shown stays.
        }
    }

    /** Stops pulling [tuner]'s channel, for everyone watching it. */
    fun stop(tuner: TunerStatus) = change(said = AdminDone(STOPPED, listOf(tuner.channelName))) {
        call("DELETE", listOf("api", "live", "tuners", tuner.channelId))
        tunersNow.value = tunersNow.value?.filterNot { it.channelId == tuner.channelId }
        readTuners()
    }

    companion object {
        const val STOPPED = "livetv.tuner.stopped"
        const val TUNERS_EVERY = 5_000L

        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer { LiveTvAdminViewModel(screensGraph().data.admin) }
        }
    }
}
