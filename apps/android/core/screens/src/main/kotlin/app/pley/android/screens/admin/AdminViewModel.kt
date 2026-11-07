package app.pley.android.screens.admin

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.pley.android.data.ServerAdmin
import app.pley.android.network.AdminApi
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch

/** What a page of managing the server read: loading, failed, or the answer. */
sealed interface AdminLoad<out T> {
    data object Loading : AdminLoad<Nothing>

    /** Reading failed; [stale] is what was shown before, kept while it is tried again. */
    data class Failed<T>(val error: Throwable, val stale: T? = null) : AdminLoad<T>

    data class Ready<T>(val value: T) : AdminLoad<T>
}

/** The answer, or what was shown before reading failed. */
val <T> AdminLoad<T>.valueOrNull: T?
    get() = when (this) {
        is AdminLoad.Ready -> value
        is AdminLoad.Failed -> stale
        AdminLoad.Loading -> null
    }

/**
 * A page of managing the server, as Settings in the web app: it reads what
 * it shows from the server when it opens ([resource]), and sends each
 * change at once ([change]), as the phone's own Settings save every choice
 * as it is made. A change the server refuses goes to [errors] (the screen
 * says why, in the server's words), and what the page shows is read again.
 * Nothing is kept on the device.
 */
abstract class AdminViewModel(protected val admin: ServerAdmin) : ViewModel() {
    private val failures = Channel<Throwable>(Channel.BUFFERED)
    private val done = Channel<AdminDone>(Channel.BUFFERED)
    private val busyNow = MutableStateFlow(0)

    /** Changes the server refused, and reads that failed after the first: for a toast. */
    val errors: Flow<Throwable> = failures.receiveAsFlow()

    /** Changes the server took, for a toast where the page says so ([change]'s `said`). */
    val confirmations: Flow<AdminDone> = done.receiveAsFlow()

    /** How many changes are on their way: the page shows it and keeps buttons from being pressed twice. */
    val busy: StateFlow<Int> = busyNow.asStateFlow()

    /**
     * What [read] answers, read now and again on [AdminResource.reload]. A
     * read that fails after an answer keeps it shown ([AdminLoad.Failed.stale]).
     */
    protected fun <T> resource(read: suspend AdminApi.() -> T): AdminResource<T> = AdminResource(this, read)

    /**
     * Sends a change with [send]; then [after] (reading again what it
     * changed). [said]: what to confirm once the server took it. Refusals
     * go to [errors] and [after] runs anyway, so the page shows what the
     * server has.
     */
    protected fun change(said: AdminDone? = null, after: () -> Unit = {}, send: suspend AdminApi.() -> Unit): Job =
        viewModelScope.launch {
            busyNow.value += 1
            try {
                admin.api().send()
                if (said != null) done.send(said)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                failures.send(e)
            } finally {
                busyNow.value -= 1
                after()
            }
        }

    internal fun failed(error: Throwable) {
        failures.trySend(error)
    }

    internal suspend fun admin(): AdminApi = admin.api()
}

/** What a [AdminViewModel.change] confirms: a key the screen has words for, with what goes in them. */
data class AdminDone(val what: String, val args: List<Any> = emptyList())

/** A thing a page shows, read from the server ([AdminViewModel.resource]). */
class AdminResource<T> internal constructor(
    private val owner: AdminViewModel,
    private val read: suspend AdminApi.() -> T,
) {
    private val load = MutableStateFlow<AdminLoad<T>>(AdminLoad.Loading)
    private var job: Job? = null

    val state: StateFlow<AdminLoad<T>> = load.asStateFlow()

    init {
        reload()
    }

    /** Reads it again; what was shown stays while it does. */
    fun reload() {
        job?.cancel()
        job = owner.viewModelScope.launch {
            try {
                load.value = AdminLoad.Ready(owner.admin().read())
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                val shown = load.value.valueOrNull
                load.value = AdminLoad.Failed(e, shown)
                if (shown != null) owner.failed(e)
            }
        }
    }

    /** Shows [value] at once (an answer a change came back with). */
    fun set(value: T) {
        job?.cancel()
        load.value = AdminLoad.Ready(value)
    }
}
