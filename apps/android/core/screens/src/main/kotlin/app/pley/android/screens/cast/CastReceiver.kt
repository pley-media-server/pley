package app.pley.android.screens.cast

import app.pley.android.data.CastRepository
import app.pley.android.model.CastAction
import app.pley.android.model.ServerEvent
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlaybackController
import app.pley.android.screens.PlayerLauncher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch

/**
 * A TV being cast to (docs/ANDROID.md, "Casting"). Video only: music and
 * Live TV are left as they are. Its rules, which the native Samsung app
 * follows too:
 *
 * 1. `cast_play`, while the TV shows its screens ([ready]: signed in, past
 *    "Who's watching?" and its PIN): the video player open leaves as Back
 *    would (its progress reported as it closes), and the player opens on
 *    the item, its version and position; no position resumes where the
 *    profile left off, without asking.
 * 2. `cast_control`: the open video player pauses, resumes, seeks, or
 *    leaves as Back does (stop), cast or not. No player open: nothing.
 * 3. While a video player is open, it says what it plays ([CastReports]),
 *    and that it plays nothing once it closes.
 *
 * Commands come only while the event stream is open: signed in, with the
 * app in the foreground. Called on the main thread; [scope] is the main one.
 */
class CastReceiver(
    private val cast: CastRepository,
    private val launcher: PlayerLauncher,
    private val scope: CoroutineScope,
    private val now: () -> Long = { System.nanoTime() / 1_000_000 },
) {
    /** The video player on screen, as the receiver steers it. */
    interface Player {
        val controller: PlaybackController

        /** Leaves the player, as Back does: its progress is reported as it closes. */
        fun exit()
    }

    /** The TV shows its screens: signed in, past "Who's watching?" and its PIN. Its shell says so. */
    val ready = MutableStateFlow(false)

    private var player: Player? = null
    private var reports: CastReports? = null
    private var following: Job? = null
    private var started = false

    /** Starts taking commands, once. */
    fun start() {
        if (started) return
        started = true
        scope.launch { cast.commands.collect(::handle) }
    }

    /** [player] shows a playback now: it is steered, and says what it plays. */
    fun opened(player: Player) {
        following?.cancel()
        this.player = player
        val said = CastReports().also { reports = it }
        following = scope.launch {
            player.controller.state.collect { state -> said.next(state, now())?.let(cast::report) }
        }
    }

    /** [player] closed: it plays nothing now, unless another player took its place. */
    fun closed(player: Player) {
        if (this.player !== player) return
        following?.cancel()
        following = null
        this.player = null
        reports?.closed()?.let(cast::report)
        reports = null
    }

    /** A command for this device. */
    fun handle(command: ServerEvent) {
        when (command) {
            is ServerEvent.CastPlay -> {
                if (!ready.value) return
                player?.exit()
                launcher.play(PlayRequest(command.itemId, command.fileId, startMs = command.positionMs))
            }
            is ServerEvent.CastControl -> {
                val open = player ?: return
                when (command.action) {
                    CastAction.Pause -> open.controller.pause()
                    CastAction.Resume -> open.controller.play()
                    CastAction.Seek -> command.positionMs?.let { open.controller.seekTo(it) }
                    CastAction.Stop -> open.exit()
                    CastAction.Unknown -> Unit
                }
            }
            else -> Unit
        }
    }
}
