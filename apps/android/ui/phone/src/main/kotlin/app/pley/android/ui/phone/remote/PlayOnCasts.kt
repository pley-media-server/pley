package app.pley.android.ui.phone.remote

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.pley.android.common.UiText
import app.pley.android.common.errorSentence
import app.pley.android.data.RemoteRepository
import app.pley.android.model.CastRequest
import app.pley.android.network.CastRefusal
import app.pley.android.network.PairingRefusal
import app.pley.android.ui.phone.R
import java.util.concurrent.atomic.AtomicLong
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.getAndUpdate
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch

/**
 * Play on…'s cast on its way, kept by the activity rather than the
 * chooser: a Google Cast or AirPlay device takes seconds to start, and
 * neither the chooser closing meanwhile nor the activity being made again
 * (a rotation, a fold, a resize, dark mode) calls the request off. One at
 * a time in a window, as the web's button allows: [starting] says on which
 * device, and a chooser opened meanwhile shows it there and waits.
 *
 * Its work holds only this, the app's [RemoteRepository] and plain data:
 * nothing of the composition or the activity that started it. What comes
 * of it reaches the screen as data the composition showing then reads:
 * [forChooser] for the chooser open now ([opened]), whichever opening of
 * it sent the cast, and [told] for its host (a cast that went, a toast
 * while no chooser is open).
 */
internal class PlayOnCasts : ViewModel() {
    private val busy = MutableStateFlow<String?>(null)
    private val news = Channel<CastNews>(Channel.UNLIMITED)
    private val pending = MutableStateFlow<CastOutcome?>(null)

    // The chooser showing now, by its opening's id; null while none is.
    private var shown: Long? = null

    /** The device a cast, or the pairing it needs, is starting on (its device id); `null` while none is. */
    val starting: StateFlow<String?> = busy.asStateFlow()

    /** What the chooser open now shows of a cast ([took] once it has). */
    val forChooser: StateFlow<CastOutcome?> = pending.asStateFlow()

    /** What the chooser's host does next ([CastNews]), kept until it reads it: across the activity being made again too. */
    val told: Flow<CastNews> = news.receiveAsFlow()

    /** The chooser's opening [id] shows: what a cast comes to is shown in it. */
    fun opened(id: Long) {
        shown = id
    }

    /** The opening [id] closed: what it hadn't shown yet is a toast. */
    fun closed(id: Long) {
        if (shown != id) return
        shown = null
        pending.getAndUpdate { null }?.toast?.let { news.trySend(CastNews.Toast(it)) }
    }

    /** The chooser showed [outcome]. */
    fun took(outcome: CastOutcome) {
        pending.compareAndSet(outcome, null)
    }

    /**
     * Sends [request] to the receiver [receiverId] of [deviceId] (which goes
     * by [name]) from the chooser's opening [from]; [pin]: the profile's PIN
     * typed for another profile's device. Whether it went out: not while a
     * cast is starting.
     */
    fun cast(remote: RemoteRepository, deviceId: String, receiverId: String, name: String, request: CastRequest, from: Long, pin: String? = null): Boolean =
        start(deviceId) { send(remote, deviceId, receiverId, name, request.copy(pin = pin), from, paired = false) }

    /** "Show it again": the AirPlay device [deviceId] shows a new code. */
    fun showCodeAgain(remote: RemoteRepository, deviceId: String, receiverId: String, name: String): Boolean =
        start(deviceId) { showCode(remote, deviceId, receiverId, name, again = true) }

    /** Pairs the server with the AirPlay device [deviceId] with the [code] it shows, then sends it [request]. */
    fun pair(remote: RemoteRepository, deviceId: String, receiverId: String, name: String, code: String, request: CastRequest, from: Long): Boolean =
        start(deviceId) {
            try {
                remote.pair(receiverId, code)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                val text = pairingText(e, name)
                when (PairingRefusal.of(e)) {
                    // A code not the one shown, or tries to wait for: the pad says so, to type again.
                    PairingRefusal.WrongCode -> show(CastOutcome.Pad(deviceId, pairing = true, error = text, shake = true, reopens = false, toast = text))
                    PairingRefusal.Wait -> show(CastOutcome.Pad(deviceId, pairing = true, error = text, reopens = false, toast = text))
                    else -> show(CastOutcome.Refused(deviceId, text))
                }
                return@start
            }
            send(remote, deviceId, receiverId, name, request, from, paired = true)
        }

    /** Runs [work] for [deviceId], unless a cast is starting already; whether it does. */
    internal fun start(deviceId: String, work: suspend () -> Unit): Boolean {
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

    private suspend fun send(remote: RemoteRepository, deviceId: String, receiverId: String, name: String, request: CastRequest, from: Long, paired: Boolean) {
        try {
            remote.cast(receiverId, request)
            news.trySend(CastNews.Sent(deviceId, name))
            // The chooser it was sent from closes; one opened since, for this or something else, stays.
            if (shown == from) pending.value = CastOutcome.Sent(deviceId, from)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            val refusal = CastRefusal.of(e, pinSent = request.pin != null)
            val text = castText(e, refusal, name)
            when (refusal) {
                // An AirPlay device the server hasn't paired with: its code first. Asked again once paired, it didn't pair.
                CastRefusal.PairingNeeded -> if (paired) {
                    show(CastOutcome.Refused(deviceId, UiText.res(R.string.play_on_pairing_failed, name)))
                } else {
                    showCode(remote, deviceId, receiverId, name, again = false)
                }
                // A PIN set since GET /api/auth/me said there was none.
                CastRefusal.PinNeeded -> show(CastOutcome.Pad(deviceId, pairing = false, toast = text))
                CastRefusal.WrongPin -> show(CastOutcome.Pad(deviceId, pairing = false, error = text, shake = true, toast = text))
                CastRefusal.PinLocked -> show(CastOutcome.Pad(deviceId, pairing = false, error = text, lockedOut = true, toast = text))
                // The server's sentence says how long to wait.
                CastRefusal.Throttled -> show(CastOutcome.Pad(deviceId, pairing = false, error = text, toast = text))
                else -> show(CastOutcome.Refused(deviceId, text))
            }
        }
    }

    /**
     * The AirPlay device [deviceId] shows a code, and the pad asks for it;
     * [again]: "Show it again" on the pad already asking. With no chooser
     * open, before or once the device answered, a code would show with no
     * pad to type it on: a toast says to pick the device again, which asks
     * for one.
     */
    private suspend fun showCode(remote: RemoteRepository, deviceId: String, receiverId: String, name: String, again: Boolean) {
        val pickAgain = CastNews.Toast(UiText.res(R.string.play_on_pair_first, name))
        if (shown == null) {
            news.trySend(pickAgain)
            return
        }
        try {
            val length = remote.showPairingCode(receiverId)
            if (shown == null) news.trySend(pickAgain) else show(CastOutcome.Pad(deviceId, pairing = true, length = length, reopens = !again, toast = pickAgain.text))
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            val text = pairingText(e, name)
            show(if (again) CastOutcome.Pad(deviceId, pairing = true, error = text, reopens = false, toast = text) else CastOutcome.Refused(deviceId, text))
        }
    }

    /** [outcome] in the chooser open now, else, when it says something, in a toast. */
    private fun show(outcome: CastOutcome) {
        if (shown != null) pending.value = outcome else outcome.toast?.let { news.trySend(CastNews.Toast(it)) }
    }

    companion object {
        private val openings = AtomicLong()

        /** An id for an opening of the chooser, its own across windows and the activity being made again. */
        fun nextOpening(): Long = openings.incrementAndGet()
    }
}

/** What a cast comes to, for the chooser open now ([PlayOnCasts.forChooser]). */
internal sealed interface CastOutcome {
    val deviceId: String

    /** What a toast says of it when the chooser closed before showing it; `null`: nothing. */
    val toast: UiText?

    /** It went: the chooser's opening [from], which sent it, closes. */
    data class Sent(override val deviceId: String, val from: Long) : CastOutcome {
        override val toast: UiText? get() = null
    }

    /** Refused: said under the devices ([text]), the pad, if any, closed. */
    data class Refused(override val deviceId: String, val text: UiText) : CastOutcome {
        override val toast: UiText get() = text
    }

    /**
     * The pad for [deviceId]: the profile's PIN, or with [pairing] the code
     * an AirPlay device shows, of [length] digits when known. [error] under
     * its dots, which [shake]; [lockedOut]: the PIN is locked. With
     * [reopens] false it answers a pad already asking: once that is closed,
     * [error] goes under the devices instead.
     */
    data class Pad(
        override val deviceId: String,
        val pairing: Boolean,
        val length: Int? = null,
        val error: UiText? = null,
        val shake: Boolean = false,
        val lockedOut: Boolean = false,
        val reopens: Boolean = true,
        override val toast: UiText?,
    ) : CastOutcome
}

/** What a cast tells the chooser's host. */
internal sealed interface CastNews {
    /** It went to [deviceId], which goes by [name]: the remote follows it. */
    data class Sent(val deviceId: String, val name: String) : CastNews

    /** What no chooser is open to say: a toast says [text]. */
    data class Toast(val text: UiText) : CastNews
}

/** What Play on… says when pairing with [name] didn't go. */
private fun pairingText(e: Exception, name: String): UiText = when (PairingRefusal.of(e)) {
    PairingRefusal.WrongCode -> UiText.res(R.string.play_on_wrong_code, name)
    PairingRefusal.Wait -> UiText.res(R.string.play_on_pairing_wait, name)
    PairingRefusal.Gone -> UiText.res(R.string.play_on_gone, name)
    PairingRefusal.Failed -> UiText.res(R.string.play_on_pairing_failed, name)
    else -> errorSentence(e)
}

/** What Play on… says when [name] refused a cast ([refusal]). */
private fun castText(e: Exception, refusal: CastRefusal, name: String): UiText = when (refusal) {
    CastRefusal.Gone -> UiText.res(R.string.play_on_gone, name)
    CastRefusal.NotCastable -> UiText.res(R.string.play_on_cant, name)
    CastRefusal.TooMany -> UiText.res(R.string.remote_too_many)
    CastRefusal.Unreachable -> UiText.res(R.string.play_on_unreachable, name)
    CastRefusal.Failed -> UiText.res(R.string.play_on_failed, name)
    CastRefusal.ConnectionsFull -> UiText.res(R.string.play_on_connections_full, name)
    CastRefusal.WrongPin -> UiText.res(R.string.play_on_wrong_pin)
    CastRefusal.PinLocked -> UiText.res(R.string.play_on_pin_locked)
    // The server's sentence: how long to wait, for one.
    else -> errorSentence(e)
}
