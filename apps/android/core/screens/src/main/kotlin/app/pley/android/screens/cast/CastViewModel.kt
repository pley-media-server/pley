@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.screens.cast

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.data.CastPlaying
import app.pley.android.data.CastRepository
import app.pley.android.data.CastSent
import app.pley.android.data.CastTarget
import app.pley.android.model.CastAction
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.screensGraph
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** Where a device's remote stands. */
sealed interface CastRemoteStatus {
    /** Sent something, and not heard playing it yet: "Waiting for <device>…". */
    data object Waiting : CastRemoteStatus

    /** Sent something [CastViewModel.ANSWER_MS] ago and not heard playing it: "<device> didn't answer". */
    data object NoAnswer : CastRemoteStatus

    /** It plays [playing]. */
    data class Playing(val playing: CastPlaying) : CastRemoteStatus

    /** It plays nothing. */
    data object Idle : CastRemoteStatus
}

/** A device's remote: the device, and where it stands. */
data class CastRemoteUi(val device: CastTarget, val status: CastRemoteStatus)

/** What went wrong with a device, for a toast. */
sealed interface CastProblem {
    /** [device] couldn't be sent the title. */
    data class NotSent(val device: String) : CastProblem

    /** [device] couldn't be steered. */
    data class NotSteered(val device: String) : CastProblem
}

/**
 * Casting from a phone (docs/ANDROID.md, "Casting"): the Cast button, the
 * "Play on" picker, a device's remote and the bar above the navigation.
 *
 * - The Cast button shows while the profile has a device to cast to
 *   ([available]).
 * - Picking a device sends it the title ([pick]), from a position (the
 *   player's) or from where the profile left off (a title's page); a
 *   device that plays something already shows its remote instead.
 * - A remote waits for the device to say it plays what it was sent, for
 *   [ANSWER_MS] at most; it then says the device didn't answer.
 * - The bar shows the device last cast to while it plays something.
 */
class CastViewModel(private val cast: CastRepository, private val launcher: PlayerLauncher) : ViewModel() {
    val devices: StateFlow<List<CastTarget>> = cast.devices

    /** The Cast button shows: the profile has a device to cast to. */
    val available: StateFlow<Boolean> =
        cast.devices.map { it.isNotEmpty() }.distinctUntilChanged().stateIn(viewModelScope, SharingStarted.Eagerly, cast.devices.value.isNotEmpty())

    /** The bar above the navigation: the device last cast to, while it plays something; `null` otherwise. */
    val bar: StateFlow<CastTarget?> = combine(cast.devices, cast.sent) { devices, sent ->
        devices.firstOrNull { it.id == sent?.deviceId }?.takeIf { it.playing != null }
    }.distinctUntilChanged().stateIn(viewModelScope, SharingStarted.Eagerly, null)

    /** A device whose remote is to show, each time one is sent something or picked while it plays. */
    val remotes: Flow<String> get() = cast.remotes

    private val shown = MutableStateFlow<String?>(null)

    /** The remote showing, if any; `null` too once its device leaves the list. */
    val remote: StateFlow<CastRemoteUi?> = combine(shown, cast.devices, cast.sent, late()) { id, devices, sent, late ->
        val device = devices.firstOrNull { it.id == id } ?: return@combine null
        CastRemoteUi(device, statusOf(device, sent, late))
    }.stateIn(viewModelScope, SharingStarted.Eagerly, null)

    private val _problem = MutableStateFlow<CastProblem?>(null)

    /** Something that went wrong, until [dismissProblem]. */
    val problem: StateFlow<CastProblem?> = _problem.asStateFlow()

    init {
        // A remote goes with its device, when that leaves the list.
        viewModelScope.launch {
            cast.devices.collect { devices -> shown.update { id -> id?.takeIf { devices.any { it.id == id } } } }
        }
    }

    /** The clock [CastPlaying.positionAt] takes. */
    fun now(): Long = cast.now()

    /**
     * "Play on" [device]: it plays [itemId] (its version [fileId]) from
     * [positionMs], `null` from where the profile left off; [sent] runs once
     * it was sent (the player leaves then). A device that plays something
     * already shows its remote instead, and [sent] runs at once.
     */
    fun pick(device: CastTarget, itemId: Long, fileId: Long? = null, positionMs: Long? = null, sent: () -> Unit = {}) {
        if (device.playing != null) {
            cast.follow(device.id)
            sent()
            return
        }
        viewModelScope.launch {
            try {
                cast.play(device.id, itemId, fileId, positionMs)
                sent()
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                _problem.value = CastProblem.NotSent(device.name)
            }
        }
    }

    fun openRemote(deviceId: String) {
        shown.value = deviceId
    }

    fun closeRemote() {
        shown.value = null
    }

    fun pause(device: CastTarget) = control(device, CastAction.Pause)

    fun resume(device: CastTarget) = control(device, CastAction.Resume)

    /** Seeks to [positionMs], within the title. */
    fun seek(device: CastTarget, positionMs: Long) {
        val playing = device.playing ?: return
        val end = playing.durationMs ?: playing.item.durationMs
        control(device, CastAction.Seek, positionMs.coerceIn(0, end?.takeIf { it > 0 } ?: Long.MAX_VALUE))
    }

    /** [deltaMs] from where it is now: back 10 s is -10 000, forward 30 s 30 000. */
    fun skip(device: CastTarget, deltaMs: Long) {
        val playing = device.playing ?: return
        seek(device, playing.positionAt(now()) + deltaMs)
    }

    fun stop(device: CastTarget) = control(device, CastAction.Stop)

    /** "Play here": the device stops, and this phone plays the title from where the device was. */
    fun playHere(device: CastTarget) {
        val playing = device.playing ?: return
        val at = playing.positionAt(now())
        control(device, CastAction.Stop)
        launcher.play(PlayRequest(playing.item.id, startMs = at))
        closeRemote()
    }

    fun dismissProblem() {
        _problem.value = null
    }

    private fun control(device: CastTarget, action: CastAction, positionMs: Long? = null) {
        viewModelScope.launch {
            try {
                cast.control(device.id, action, positionMs)
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                _problem.value = CastProblem.NotSteered(device.name)
            }
        }
    }

    /** Whether the last title sent went [ANSWER_MS] without an answer: false until then. */
    private fun late(): Flow<Boolean> = cast.sent.flatMapLatest { sent ->
        if (sent?.itemId == null || sent.answered) {
            flowOf(false)
        } else {
            flow {
                emit(false)
                val left = ANSWER_MS - (cast.now() - sent.atMs)
                if (left > 0) delay(left)
                emit(true)
            }
        }
    }

    companion object {
        /** How long a device has to say it plays what it was sent before its remote says it didn't answer. */
        const val ANSWER_MS = 15_000L

        /** Where [device]'s remote stands, [sent] being what this phone sent last, [late] its answer late. */
        fun statusOf(device: CastTarget, sent: CastSent?, late: Boolean): CastRemoteStatus {
            val waiting = sent != null && sent.deviceId == device.id && sent.itemId != null && !sent.answered
            val playing = device.playing
            return when {
                waiting && late -> CastRemoteStatus.NoAnswer
                waiting -> CastRemoteStatus.Waiting
                playing != null -> CastRemoteStatus.Playing(playing)
                else -> CastRemoteStatus.Idle
            }
        }

        val Factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val graph = screensGraph()
                CastViewModel(graph.data.cast, graph.playerLauncher)
            }
        }
    }
}
