package app.pley.android.screens.requests

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.data.RequestsRepository
import app.pley.android.data.RequestsState
import app.pley.android.data.summary
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

/**
 * The ways into requests, for the shells' menus and the
 * search screens: Discover, and Your requests (Requests for managers)
 * carrying one number, in the danger colour while failed requests wait
 * for a manager.
 */
data class RequestsEntry(
    /** Discover shows: requests are on and open to this account (perhaps only to say "Sign in again"). */
    val discover: Boolean = false,
    /** Your requests shows, or for managers Requests. */
    val requests: Boolean = false,
    /** It's the managers' Requests, opening on Needs attention. */
    val manage: Boolean = false,
    /** The number on the way into requests; 0 for none. */
    val badge: Int = 0,
    /** Managers: of [badge], the failed requests waiting for one. */
    val failed: Int = 0,
) {
    /** The number is in the danger colour. */
    val alarm: Boolean get() = failed > 0

    companion object {
        val None = RequestsEntry()

        fun of(state: RequestsState): RequestsEntry {
            val summary = state.summary ?: return None
            val requests = summary.requestsEntry
            return RequestsEntry(
                discover = summary.discover,
                requests = requests,
                manage = requests && summary.manage,
                badge = if (requests) summary.badge else 0,
                failed = if (requests && summary.manage) summary.failed else 0,
            )
        }
    }
}

/** [RequestsEntry] as the summary changes, for whichever screen shows the ways in. */
class RequestsEntryViewModel(requests: RequestsRepository) : ViewModel() {
    val entry: StateFlow<RequestsEntry> = requests.state
        .map(RequestsEntry::of)
        .distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_MS), RequestsEntry.of(requests.state.value))

    companion object {
        /** Keeps following this long after the screen stops showing. */
        const val STOP_MS = 5_000L

        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer { RequestsEntryViewModel(screensGraph().data.requests) }
        }
    }
}
