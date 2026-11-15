package app.pley.android.ui.phone.remote

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.pley.android.common.UiText
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch

/**
 * Play on…'s cast on its way, kept by the activity rather than the
 * chooser: a Google Cast or AirPlay device takes seconds to start, and
 * neither the chooser closing meanwhile nor the activity being made again
 * (a rotation, a fold, a resize, dark mode) calls the request off. One at
 * a time, as the web's button allows: [starting] says on which device, and
 * a chooser opened meanwhile shows it there and waits.
 */
internal class PlayOnCasts : ViewModel() {
    private val busy = MutableStateFlow<String?>(null)
    private val news = Channel<CastNews>(Channel.UNLIMITED)

    /** The device a cast, or the pairing it needs, is starting on (its device id); `null` while none is. */
    val starting: StateFlow<String?> = busy.asStateFlow()

    /** What the chooser's host does next ([CastNews]), kept until it reads it: across the activity being made again too. */
    val told: Flow<CastNews> = news.receiveAsFlow()

    /** Runs [work] for [deviceId], unless a cast is starting already; whether it does. */
    fun start(deviceId: String, work: suspend () -> Unit): Boolean {
        if (!busy.compareAndSet(null, deviceId)) return false
        viewModelScope.launch {
            try {
                work()
            } finally {
                busy.value = null
            }
        }
        return true
    }

    fun tell(what: CastNews) {
        news.trySend(what)
    }
}

/** What a cast tells the chooser's host. */
internal sealed interface CastNews {
    /** It went to [deviceId], which goes by [name]: the remote follows it. */
    data class Sent(val deviceId: String, val name: String) : CastNews

    /** It didn't, or needs the device picked again, and the chooser that would say so is gone: a toast says [text]. */
    data class Refused(val text: UiText) : CastNews
}
